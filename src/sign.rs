use std::fmt;

pub trait Signer: Send + Sync {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &[]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignError {
    InvalidConfiguration(String),
    PdfLoadFailed(String),
    SignatureFieldNotFound(String),
    InvalidSignatureField(String),
    SigningFailed(String),
    NotImplemented(String),
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => write!(f, "Invalid signing configuration: {message}"),
            Self::PdfLoadFailed(message) => write!(f, "PDF load failed: {message}"),
            Self::SignatureFieldNotFound(field) => write!(f, "Signature field '{field}' not found"),
            Self::InvalidSignatureField(field) => write!(f, "Field '{field}' is not a signature field"),
            Self::SigningFailed(message) => write!(f, "Signing failed: {message}"),
            Self::NotImplemented(message) => write!(f, "Signing is not implemented: {message}"),
        }
    }
}

impl std::error::Error for SignError {}

#[cfg(not(target_arch = "wasm32"))]
pub struct Pkcs12Signer {
    inner: pdfluent_sign::Pkcs12Signer,
}

#[cfg(not(target_arch = "wasm32"))]
impl Pkcs12Signer {
    pub fn from_pkcs12_bytes(data: &[u8], password: &str) -> Result<Self, SignError> {
        let inner = pdfluent_sign::Pkcs12Signer::from_pkcs12(data, password)
            .map_err(|e| SignError::SigningFailed(format!("{e:?}")))?;
        use pdfluent_sign::PdfSigner as BackendSigner;
        if BackendSigner::digest_algorithm(&inner) != pdfluent_sign::DigestAlgorithm::Sha256
            || BackendSigner::signature_algorithm_oid(&inner)
                != [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]
        {
            return Err(SignError::NotImplemented(
                "PKCS#12 signing currently supports RSA PKCS#1 v1.5 with SHA-256".into(),
            ));
        }
        Ok(Self { inner })
    }

    pub fn from_pkcs12_file(path: impl AsRef<std::path::Path>, password: &str) -> Result<Self, SignError> {
        let data = std::fs::read(path.as_ref())
            .map_err(|e| SignError::SigningFailed(e.to_string()))?;
        Self::from_pkcs12_bytes(&data, password)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Signer for Pkcs12Signer {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError> {
        use pdfluent_sign::PdfSigner as BackendSigner;
        BackendSigner::sign(&self.inner, data)
            .map_err(|e| SignError::SigningFailed(format!("{e:?}")))
    }

    fn certificate_chain(&self) -> &[Vec<u8>] {
        use pdfluent_sign::PdfSigner as BackendSigner;
        BackendSigner::certificate_chain_der(&self.inner)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct SignerAdapter<'a> {
    signer: &'a dyn Signer,
}

#[cfg(not(target_arch = "wasm32"))]
impl pdfluent_sign::PdfSigner for SignerAdapter<'_> {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, pdfluent_sign::SignError> {
        self.signer.sign(data).map_err(|e| pdfluent_sign::SignError::CmsBuild(e.to_string()))
    }

    fn certificate_chain_der(&self) -> &[Vec<u8>] {
        self.signer.certificate_chain()
    }

    fn digest_algorithm(&self) -> pdfluent_sign::DigestAlgorithm {
        pdfluent_sign::DigestAlgorithm::Sha256
    }

    fn signature_algorithm_oid(&self) -> &[u8] {
        const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
        OID_SHA256_WITH_RSA
    }
}

pub struct PdfSigner {
    field: Option<String>,
    signer: Option<Box<dyn Signer>>,
    reason: Option<String>,
    location: Option<String>,
    contact: Option<String>,
    placeholder_size: usize,
}

impl PdfSigner {
    pub fn new() -> Self {
        Self {
            field: None,
            signer: None,
            reason: None,
            location: None,
            contact: None,
            placeholder_size: 8192,
        }
    }

    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn signer<S>(mut self, signer: S) -> Self
    where
        S: Signer + 'static,
    {
        self.signer = Some(Box::new(signer));
        self
    }

    pub fn reason(mut self, value: impl Into<String>) -> Self {
        self.reason = Some(value.into());
        self
    }

    pub fn location(mut self, value: impl Into<String>) -> Self {
        self.location = Some(value.into());
        self
    }

    pub fn contact(mut self, value: impl Into<String>) -> Self {
        self.contact = Some(value.into());
        self
    }

    pub fn placeholder_size(mut self, value: usize) -> Self {
        self.placeholder_size = value.max(1024);
        self
    }

    pub fn sign(&self, pdf: &[u8]) -> Result<Vec<u8>, SignError> {
        let field_name = self.field.as_deref().ok_or_else(|| {
            SignError::InvalidConfiguration("signature field is required".into())
        })?;
        let signer = self.signer.as_ref().ok_or_else(|| {
            SignError::InvalidConfiguration("signer is required".into())
        })?;

        let doc = lopdf::Document::load_mem(pdf)
            .map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
        let fields = super::collect_fields(&doc);
        if let Some((_, _, field_type)) = fields.get(field_name) {
            if field_type.as_slice() != b"Sig" {
                return Err(SignError::InvalidSignatureField(field_name.into()));
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = (pdf, signer);
            return Err(SignError::NotImplemented(
                "Digital signing is not available in the WASM target yet".into(),
            ));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if signer.certificate_chain().is_empty() {
                return Err(SignError::InvalidConfiguration(
                    "signer certificate chain is required".into(),
                ));
            }

            let adapter = SignerAdapter { signer: signer.as_ref() };
            let options = pdfluent_sign::SignOptions {
                reason: self.reason.clone(),
                location: self.location.clone(),
                contact: self.contact.clone(),
                field_name: Some(field_name.to_string()),
                visible_rect: None,
                sub_filter: pdfluent_sign::SubFilter::EtsiCadesDetached,
                certification: None,
                placeholder_size: self.placeholder_size,
            };

            pdfluent_sign::sign_pdf_incremental(pdf, &adapter, &options)
                .map_err(|e| SignError::SigningFailed(format!("{e:?}")))
        }
    }
}

impl Default for PdfSigner {
    fn default() -> Self {
        Self::new()
    }
}

[executed on device: QuyenLe (dc1d89ef-2452-4cf0-af98-88586f0bd77d)]