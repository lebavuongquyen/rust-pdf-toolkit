#[cfg(not(target_arch = "wasm32"))]
#[path = "sign_native.rs"]
mod native;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    CertificateSigner, CmsSignatureMode, EcdsaSigner, PdfSigner, SignError, Signer,
    fill_and_sign_pdf,
};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::fmt;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CmsSignatureMode {
        SignedAttributesRsaPkcs1Sha256,
        DirectEcdsaSha256,
    }

    pub trait Signer: Send + Sync {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SignError {
        NotImplemented(String),
    }

    impl fmt::Display for SignError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::NotImplemented(message) => write!(f, "Signing is not implemented: {message}"),
            }
        }
    }

    impl std::error::Error for SignError {}

    pub struct CertificateSigner;

    impl CertificateSigner {
        pub fn from_pkcs8_der(
            _certificate_der: impl Into<Vec<u8>>,
            _private_key_der: &[u8],
        ) -> Result<Self, SignError> {
            Err(SignError::NotImplemented(
                "Digital signing is native-only at this stage".into(),
            ))
        }
    }

    pub struct EcdsaSigner;

    impl EcdsaSigner {
        pub fn from_pkcs8_der(
            _certificate_der: impl Into<Vec<u8>>,
            _private_key_der: &[u8],
        ) -> Result<Self, SignError> {
            Err(SignError::NotImplemented(
                "Digital signing is native-only at this stage".into(),
            ))
        }
    }

    pub struct PdfSigner;

    impl PdfSigner {
        pub fn new() -> Self {
            Self
        }

        pub fn field(self, _field: impl Into<String>) -> Self {
            self
        }

        pub fn signer<S: Signer + 'static>(self, _signer: S) -> Self {
            self
        }

        pub fn reason(self, _value: impl Into<String>) -> Self {
            self
        }

        pub fn location(self, _value: impl Into<String>) -> Self {
            self
        }

        pub fn contact(self, _value: impl Into<String>) -> Self {
            self
        }

        pub fn placeholder_size(self, _value: usize) -> Self {
            self
        }

        pub fn flatten(self, _value: bool) -> Self {
            self
        }

        pub fn appearance(self, _options: crate::appearance::SignatureAppearanceOptions) -> Self {
            self
        }

        pub fn piece_info(self, _value: serde_json::Value) -> Self {
            self
        }

        pub fn piece_info_json(self, _json_str: &str) -> Result<Self, SignError> {
            Ok(self)
        }

        pub fn locked_piece_info(
            self,
            _app_name: impl Into<String>,
            _data: serde_json::Value,
            _secret_key: impl Into<String>,
        ) -> Self {
            self
        }

        pub fn design(self, _design: crate::appearance::SignatureDesign) -> Self {
            self
        }

        pub fn validate(&self, _pdf: &[u8]) -> Result<(), SignError> {
            Err(SignError::NotImplemented(
                "Digital signing is native-only at this stage".into(),
            ))
        }

        pub fn sign(&self, _pdf: &[u8]) -> Result<Vec<u8>, SignError> {
            Err(SignError::NotImplemented(
                "Digital signing is native-only at this stage".into(),
            ))
        }

        pub fn fill_and_sign(
            &self,
            _template: &[u8],
            _json: &str,
        ) -> Result<(Vec<u8>, crate::FillReport), SignError> {
            Err(SignError::NotImplemented(
                "Digital signing is native-only at this stage".into(),
            ))
        }
    }

    pub fn fill_and_sign_pdf(
        _template: &[u8],
        _json: &str,
        _signer: &PdfSigner,
    ) -> Result<(Vec<u8>, crate::FillReport), SignError> {
        Err(SignError::NotImplemented(
            "Digital signing is native-only at this stage".into(),
        ))
    }

    impl Default for PdfSigner {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    CertificateSigner, CmsSignatureMode, EcdsaSigner, PdfSigner, SignError, Signer,
    fill_and_sign_pdf,
};
