use ::time::{OffsetDateTime, format_description::parse_borrowed};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, StringFormat};
use p384::ecdsa::SigningKey as EcdsaSigningKey;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::traits::PublicKeyParts;
use sha2::{Digest, Sha256};
use signature::{SignatureEncoding, Signer as CryptoSigner, hazmat::PrehashSigner};
use std::fmt;
use x509_parser::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmsSignatureMode {
    SignedAttributesRsaPkcs1Sha256,
    DirectEcdsaSha256,
}

pub trait Signer: Send + Sync {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &[]
    }

    fn cms_signature_mode(&self) -> CmsSignatureMode;
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
            Self::InvalidConfiguration(message) => {
                write!(f, "Invalid signing configuration: {message}")
            }
            Self::PdfLoadFailed(message) => write!(f, "PDF load failed: {message}"),
            Self::SignatureFieldNotFound(field) => write!(f, "Signature field '{field}' not found"),
            Self::InvalidSignatureField(field) => {
                write!(f, "Field '{field}' is not a signature field")
            }
            Self::SigningFailed(message) => write!(f, "Signing failed: {message}"),
            Self::NotImplemented(message) => write!(f, "Signing is not implemented: {message}"),
        }
    }
}

impl std::error::Error for SignError {}

pub struct CertificateSigner {
    private_key: RsaPrivateKey,
    certificates: Vec<Vec<u8>>,
}

impl CertificateSigner {
    pub fn from_pkcs8_der(
        certificate_der: impl Into<Vec<u8>>,
        private_key_der: &[u8],
    ) -> Result<Self, SignError> {
        let private_key = RsaPrivateKey::from_pkcs8_der(private_key_der).map_err(|e| {
            SignError::SigningFailed(format!("Invalid PKCS#8 RSA private key: {e}"))
        })?;
        Self::from_parts(certificate_der.into(), private_key)
    }

    pub fn from_parts(
        certificate_der: Vec<u8>,
        private_key: RsaPrivateKey,
    ) -> Result<Self, SignError> {
        if certificate_der.is_empty() {
            return Err(SignError::InvalidConfiguration(
                "certificate is required".into(),
            ));
        }
        if private_key.n().bits() < 2048 {
            return Err(SignError::InvalidConfiguration(
                "RSA key must be at least 2048 bits".into(),
            ));
        }
        parse_certificate(&certificate_der)?;
        Ok(Self {
            private_key,
            certificates: vec![certificate_der],
        })
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificates[0]
    }
}

impl Signer for CertificateSigner {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError> {
        let key = SigningKey::<Sha256>::new(self.private_key.clone());
        Ok(key.sign(data).to_vec())
    }

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &self.certificates
    }

    fn cms_signature_mode(&self) -> CmsSignatureMode {
        CmsSignatureMode::SignedAttributesRsaPkcs1Sha256
    }
}

pub struct EcdsaSigner {
    private_key: EcdsaSigningKey,
    certificates: Vec<Vec<u8>>,
}

impl EcdsaSigner {
    pub fn from_pkcs8_der(
        certificate_der: impl Into<Vec<u8>>,
        private_key_der: &[u8],
    ) -> Result<Self, SignError> {
        let private_key = EcdsaSigningKey::from_pkcs8_der(private_key_der).map_err(|e| {
            SignError::SigningFailed(format!("Invalid PKCS#8 P-384 private key: {e}"))
        })?;
        Self::from_parts(certificate_der.into(), private_key)
    }

    pub fn from_parts(
        certificate_der: Vec<u8>,
        private_key: EcdsaSigningKey,
    ) -> Result<Self, SignError> {
        if certificate_der.is_empty() {
            return Err(SignError::InvalidConfiguration(
                "certificate is required".into(),
            ));
        }
        parse_ecdsa_certificate(&certificate_der)?;
        Ok(Self {
            private_key,
            certificates: vec![certificate_der],
        })
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificates[0]
    }
}

impl Signer for EcdsaSigner {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError> {
        let digest = Sha256::digest(data);
        let signature: p384::ecdsa::Signature = self
            .private_key
            .sign_prehash(&digest)
            .map_err(|e| SignError::SigningFailed(format!("ECDSA signing failed: {e}")))?;
        Ok(signature.to_der().as_bytes().to_vec())
    }

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &self.certificates
    }

    fn cms_signature_mode(&self) -> CmsSignatureMode {
        CmsSignatureMode::DirectEcdsaSha256
    }
}

pub struct PdfSigner {
    field: Option<String>,
    signer: Option<Box<dyn Signer>>,
    reason: Option<String>,
    location: Option<String>,
    contact: Option<String>,
    placeholder_size: usize,
    flatten: bool,
    appearance: Option<crate::appearance::SignatureAppearanceOptions>,
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
            flatten: false,
            appearance: None,
        }
    }

    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn flatten(mut self, value: bool) -> Self {
        self.flatten = value;
        self
    }

    pub fn appearance(mut self, options: crate::appearance::SignatureAppearanceOptions) -> Self {
        self.appearance = Some(options);
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

    pub fn validate(&self, pdf: &[u8]) -> Result<(), SignError> {
        let field_name = self
            .field
            .as_deref()
            .ok_or_else(|| SignError::InvalidConfiguration("signature field is required".into()))?;
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| SignError::InvalidConfiguration("signer is required".into()))?;
        if signer.certificate_chain().is_empty() {
            return Err(SignError::InvalidConfiguration(
                "signer certificate chain is required".into(),
            ));
        }
        let doc = Document::load_mem(pdf).map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
        let fields = crate::collect_fields(&doc);
        let Some((_, field, field_type)) = fields.get(field_name) else {
            return Err(SignError::SignatureFieldNotFound(field_name.into()));
        };
        if field_type.as_slice() != b"Sig" {
            return Err(SignError::InvalidSignatureField(field_name.into()));
        }
        if field.get(b"V").is_ok() {
            return Err(SignError::InvalidSignatureField(format!("{field_name} is already signed")));
        }
        Ok(())
    }

    pub fn sign(&self, pdf: &[u8]) -> Result<Vec<u8>, SignError> {
        self.validate(pdf)?;
        let field_name = self
            .field
            .as_deref()
            .ok_or_else(|| SignError::InvalidConfiguration("signature field is required".into()))?;
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| SignError::InvalidConfiguration("signer is required".into()))?;

        if signer.certificate_chain().is_empty() {
            return Err(SignError::InvalidConfiguration(
                "signer certificate chain is required".into(),
            ));
        }

        let mut base_pdf = pdf.to_vec();
        if self.flatten || self.appearance.is_some() {
            let mut doc = Document::load_mem(&base_pdf).map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
            if self.flatten {
                crate::flatten_form_fields(&mut doc, true)
                    .map_err(SignError::SigningFailed)?;
            }
            if let Some(app_opts) = &self.appearance {
                let fields = crate::collect_fields(&doc);
                if let Some((field_id, field, _)) = fields.get(field_name) {
                    let widgets = crate::widget_ids(&doc, *field_id, field);
                    for widget_id in widgets {
                        let widget = doc
                            .get_object(widget_id)
                            .map_err(|e| SignError::PdfLoadFailed(e.to_string()))?
                            .as_dict()
                            .map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
                        let (bw, bh) = crate::rect(widget).map_err(SignError::PdfLoadFailed)?;
                        let appearance_id = crate::appearance_renderer::render_signature_appearance(
                            &mut doc,
                            bw,
                            bh,
                            app_opts,
                        )
                        .map_err(SignError::SigningFailed)?;
                        let widget_mut = doc
                            .get_object_mut(widget_id)
                            .map_err(|e| SignError::PdfLoadFailed(e.to_string()))?
                            .as_dict_mut()
                            .map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
                        crate::appearance_renderer::replace_appearance(widget_mut, appearance_id);
                    }
                }
            }
            base_pdf = crate::save_document(&mut doc).map_err(SignError::SigningFailed)?;
        }

        let doc = Document::load_mem(&base_pdf).map_err(|e| SignError::PdfLoadFailed(e.to_string()))?;
        let fields = crate::collect_fields(&doc);
        let Some((field_id, field, field_type)) = fields.get(field_name) else {
            return Err(SignError::SignatureFieldNotFound(field_name.into()));
        };
        if field_type.as_slice() != b"Sig" {
            return Err(SignError::InvalidSignatureField(field_name.into()));
        }

        let widgets = crate::widget_ids(&doc, *field_id, field);

        let mut incremental = IncrementalDocument::create_from(base_pdf, doc);
        incremental
            .opt_clone_object_to_new_document(*field_id)
            .map_err(|e| SignError::SigningFailed(e.to_string()))?;

        for wid in &widgets {
            if *wid != *field_id {
                let _ = incremental.opt_clone_object_to_new_document(*wid);
            }
        }

        let signature_id = next_object_id(&incremental.new_document);
        let contents_len = self.placeholder_size;
        let placeholder = vec![0xAA; contents_len];

        let mut signature = Dictionary::new();
        signature.set("Type", "Sig");
        signature.set("Filter", "Adobe.PPKLite");
        signature.set("SubFilter", "adbe.pkcs7.detached");
        signature.set(
            "ByteRange",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(i64::MAX),
                Object::Integer(i64::MAX),
                Object::Integer(i64::MAX),
            ]),
        );
        signature.set(
            "Contents",
            Object::String(placeholder, StringFormat::Hexadecimal),
        );
        if let Some(reason) = &self.reason {
            signature.set("Reason", pdf_text(reason));
        }
        if let Some(location) = &self.location {
            signature.set("Location", pdf_text(location));
        }
        if let Some(contact) = &self.contact {
            signature.set("ContactInfo", pdf_text(contact));
        }

        incremental
            .new_document
            .objects
            .insert(signature_id, Object::Dictionary(signature));

        let field_object = incremental
            .new_document
            .objects
            .get_mut(field_id)
            .ok_or_else(|| SignError::SignatureFieldNotFound(field_name.into()))?;
        let field_dict = field_object
            .as_dict_mut()
            .map_err(|_| SignError::InvalidSignatureField(field_name.into()))?;
        field_dict.set("V", Object::Reference(signature_id));
        let ff = field_dict.get(b"Ff").ok().and_then(|x| x.as_i64().ok()).unwrap_or(0);
        field_dict.set("Ff", Object::Integer(ff | 1));

        let mut lock_dict = Dictionary::new();
        lock_dict.set("Type", "SigFieldLock");
        lock_dict.set("Action", "All");
        field_dict.set("Lock", Object::Dictionary(lock_dict));

        for wid in &widgets {
            if let Some(w_obj) = incremental.new_document.objects.get_mut(wid) {
                if let Ok(w_dict) = w_obj.as_dict_mut() {
                    let f = w_dict.get(b"F").ok().and_then(|x| x.as_i64().ok()).unwrap_or(0);
                    w_dict.set("F", Object::Integer(f | 1 | 64));
                }
            }
        }

        let mut unsigned = Vec::new();
        incremental
            .save_to(&mut unsigned)
            .map_err(|e| SignError::SigningFailed(e.to_string()))?;

        let contents_marker = "AA".repeat(contents_len);
        let marker = contents_marker.as_bytes();
        let contents_hex_start = find_unique(&unsigned, marker).ok_or_else(|| {
            SignError::SigningFailed("signature contents placeholder not found".into())
        })?;
        let contents_start = contents_hex_start - 1;
        let contents_end = contents_hex_start + marker.len() + 1;

        let byte_range = [
            0usize,
            contents_start,
            contents_end,
            unsigned.len() - contents_end,
        ];

        let mut output = unsigned;
        replace_ascii_integer(&mut output, i64::MAX, byte_range[1] as i64)?;
        replace_ascii_integer(&mut output, i64::MAX, byte_range[2] as i64)?;
        replace_ascii_integer(&mut output, i64::MAX, byte_range[3] as i64)?;

        let digest_input = [&output[..byte_range[1]], &output[byte_range[2]..]].concat();
        let digest = Sha256::digest(&digest_input);

        let (signed_attributes, signature_value) = match signer.cms_signature_mode() {
            CmsSignatureMode::SignedAttributesRsaPkcs1Sha256 => {
                let signed_attributes = build_signed_attributes(&digest)?;
                let signature_value = signer.sign(&signed_attributes)?;
                (Some(signed_attributes), signature_value)
            }
            CmsSignatureMode::DirectEcdsaSha256 => {
                let signature_value = signer.sign(&digest_input)?;
                (None, signature_value)
            }
        };
        let cms = build_cms(
            signed_attributes.as_deref(),
            &signature_value,
            signer.certificate_chain(),
            signer.cms_signature_mode(),
        )?;

        if cms.len() > contents_len {
            return Err(SignError::SigningFailed(format!(
                "CMS signature is {} bytes but placeholder is {} bytes",
                cms.len(),
                contents_len
            )));
        }

        let hex = hex_encode(&cms);
        let mut padded_hex = vec![b'0'; marker.len()];
        padded_hex[..hex.len()].copy_from_slice(&hex);
        output[contents_hex_start..contents_hex_start + marker.len()].copy_from_slice(&padded_hex);

        Ok(output)
    }

    pub fn fill_and_sign(
        &self,
        template: &[u8],
        json: &str,
    ) -> Result<(Vec<u8>, crate::FillReport), SignError> {
        let (filled, report) = crate::fill_pdf_with_options(
            template,
            json,
            &crate::FillOptions { flatten: false },
        )
        .map_err(|e| SignError::InvalidConfiguration(format!("Fill step failed: {e}")))?;

        let signed = self.sign(&filled)?;
        Ok((signed, report))
    }
}

pub fn fill_and_sign_pdf(
    template: &[u8],
    json: &str,
    signer: &PdfSigner,
) -> Result<(Vec<u8>, crate::FillReport), SignError> {
    signer.fill_and_sign(template, json)
}

impl Default for PdfSigner {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_ecdsa_certificate(data: &[u8]) -> Result<(), SignError> {
    let (_, cert) = X509Certificate::from_der(data)
        .map_err(|e| SignError::SigningFailed(format!("Invalid X.509 certificate: {e}")))?;
    let algorithm = &cert.tbs_certificate.subject_pki.algorithm;
    if algorithm.algorithm.to_id_string() != "1.2.840.10045.2.1" {
        return Err(SignError::InvalidConfiguration(
            "ECDSA signer requires an id-ecPublicKey certificate".into(),
        ));
    }
    let Some(parameters) = &algorithm.parameters else {
        return Err(SignError::InvalidConfiguration(
            "ECDSA certificate is missing its curve parameters".into(),
        ));
    };
    if parameters.as_bytes() != [0x2B, 0x81, 0x04, 0x00, 0x22] {
        return Err(SignError::InvalidConfiguration(
            "ECDSA signer currently requires the P-384 curve".into(),
        ));
    }
    Ok(())
}

fn parse_certificate(data: &[u8]) -> Result<(), SignError> {
    let (_, cert) = X509Certificate::from_der(data)
        .map_err(|e| SignError::SigningFailed(format!("Invalid X.509 certificate: {e}")))?;
    if cert
        .tbs_certificate
        .subject_pki
        .algorithm
        .algorithm
        .to_id_string()
        != "1.2.840.113549.1.1.1"
    {
        return Err(SignError::NotImplemented(
            "Only RSA certificates are currently supported".into(),
        ));
    }
    Ok(())
}

fn next_object_id(doc: &Document) -> ObjectId {
    let max_id = doc.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);
    (max_id + 1, 0)
}

fn pdf_text(value: &str) -> Object {
    let mut bytes = vec![0xfe, 0xff];
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn find_unique(data: &[u8], needle: &[u8]) -> Option<usize> {
    let mut found = None;
    let mut start = 0;
    while let Some(relative) = data[start..]
        .windows(needle.len())
        .position(|w| w == needle)
    {
        let pos = start + relative;
        if found.is_some() {
            return None;
        }
        found = Some(pos);
        start = pos + 1;
    }
    found
}

fn replace_ascii_integer(data: &mut [u8], old: i64, new: i64) -> Result<(), SignError> {
    let old_text = old.to_string();
    let new_text = new.to_string();
    if new_text.len() > old_text.len() {
        return Err(SignError::SigningFailed(
            "ByteRange value exceeds placeholder width".into(),
        ));
    }
    let needle = old_text.as_bytes();
    let pos = data
        .windows(needle.len())
        .position(|w| w == needle)
        .ok_or_else(|| SignError::SigningFailed("ByteRange placeholder not found".into()))?;
    let mut replacement = vec![b'0'; needle.len()];
    let start = replacement.len() - new_text.len();
    replacement[start..].copy_from_slice(new_text.as_bytes());
    data[pos..pos + needle.len()].copy_from_slice(&replacement);
    Ok(())
}

fn hex_encode(data: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(data.len() * 2);
    for byte in data {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0f) as usize]);
    }
    out
}

fn der_len(len: usize) -> Vec<u8> {
    if len < 128 {
        return vec![len as u8];
    }
    let bytes = len.to_be_bytes();
    let first = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    let body = &bytes[first..];
    let mut out = vec![0x80 | body.len() as u8];
    out.extend_from_slice(body);
    out
}

fn der(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&der_len(body.len()));
    out.extend_from_slice(body);
    out
}

fn der_sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    let body = concat(parts);
    der(0x30, &body)
}

fn der_set(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut sorted = parts.to_vec();
    sorted.sort();
    let body = concat(&sorted);
    der(0x31, &body)
}

fn der_oid(oid: &[u8]) -> Vec<u8> {
    der(0x06, oid)
}

fn der_null() -> Vec<u8> {
    vec![0x05, 0x00]
}

fn der_octet_string(data: &[u8]) -> Vec<u8> {
    der(0x04, data)
}

fn der_integer(data: &[u8]) -> Vec<u8> {
    let mut value = data.to_vec();
    while value.len() > 1 && value[0] == 0 {
        value.remove(0);
    }
    if value.first().is_some_and(|b| b & 0x80 != 0) {
        value.insert(0, 0);
    }
    der(0x02, &value)
}

fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    let total = parts.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(total);
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

fn attribute(oid: &[u8], value: Vec<u8>) -> Vec<u8> {
    der_sequence(&[der_oid(oid), der_set(&[value])])
}

fn build_signed_attributes(digest: &[u8]) -> Result<Vec<u8>, SignError> {
    let content_type = attribute(
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03],
        der_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01]),
    );
    let signing_time = OffsetDateTime::now_utc()
        .format(
            &parse_borrowed::<3>("[year][month][day][hour][minute][second]Z").map_err(|e| {
                SignError::SigningFailed(format!("Signing time format failed: {e}"))
            })?,
        )
        .map_err(|e| SignError::SigningFailed(format!("Signing time generation failed: {e}")))?;
    let signing_time = attribute(
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05],
        der(0x18, signing_time.as_bytes()),
    );
    let message_digest = attribute(
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04],
        der_octet_string(digest),
    );
    Ok(der_set(&[content_type, signing_time, message_digest]))
}

fn build_cms(
    signed_attributes_set: Option<&[u8]>,
    signature: &[u8],
    certificates: &[Vec<u8>],
    mode: CmsSignatureMode,
) -> Result<Vec<u8>, SignError> {
    let cert = certificates.first().ok_or_else(|| {
        SignError::InvalidConfiguration("at least one certificate is required".into())
    })?;
    let (_, x509) = X509Certificate::from_der(cert)
        .map_err(|e| SignError::SigningFailed(format!("Invalid signer certificate: {e}")))?;

    let digest_algorithm = der_sequence(&[
        der_oid(&[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01]),
        der_null(),
    ]);
    let signature_algorithm = match mode {
        CmsSignatureMode::SignedAttributesRsaPkcs1Sha256 => der_sequence(&[
            der_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]),
            der_null(),
        ]),
        CmsSignatureMode::DirectEcdsaSha256 => der_sequence(&[
            der_oid(&[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01]),
            der_null(),
        ]),
    };

    let issuer_and_serial = der_sequence(&[
        x509.tbs_certificate.issuer.as_raw().to_vec(),
        der_integer(x509.tbs_certificate.raw_serial()),
    ]);

    let mut signer_info_parts = vec![
        vec![0x02, 0x01, 0x01],
        issuer_and_serial,
        digest_algorithm.clone(),
    ];
    if let Some(signed_attributes_set) = signed_attributes_set {
        let mut implicit = signed_attributes_set.to_vec();
        implicit[0] = 0xA0;
        signer_info_parts.push(implicit);
    }
    signer_info_parts.push(signature_algorithm);
    signer_info_parts.push(der_octet_string(signature));
    let signer_info = der_sequence(&signer_info_parts);

    let certificates_body = concat(
        &certificates
            .iter()
            .map(|cert| cert.clone())
            .collect::<Vec<_>>(),
    );
    let certificates_field = der(0xA0, &certificates_body);

    let signed_data = der_sequence(&[
        vec![0x02, 0x01, 0x01],
        der_set(&[digest_algorithm]),
        der_sequence(&[der_oid(&[
            0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01,
        ])]),
        certificates_field,
        der_set(&[signer_info]),
    ]);

    Ok(der_sequence(&[
        der_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02]),
        der(0xA0, &signed_data),
    ]))
}
