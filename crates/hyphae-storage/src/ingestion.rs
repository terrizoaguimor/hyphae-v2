// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Ingestion bridge — signed-at-source provenance INTO the journal
//! (ADR-0037).
//!
//! The hash chain (ADR-0003), head anchor (0032), ledger (0033),
//! witness (0034), and keyring (0035) all attest integrity *from the
//! journal forward*: a stored fragment was not altered after it was
//! written. They say nothing about *how the fragment got there* — its
//! payload could be fabricated, and the chain will faithfully prove the
//! fabrication was never touched. ADR-0034 named this the one
//! genuinely-open boundary: provenance *into* the store.
//!
//! This module is the ingestion-side peer of the integrity chain. At
//! admission time an [`IngestionAsserter`] — the ingestion source,
//! holding a key of its own — signs an [`IngestionCredential`] binding
//! a fragment's exact bytes to a claimed origin: a source document
//! (fixed by its hash), a locator (the byte range the fragment was
//! extracted from), and the asserter's identity. The credential is
//! itself appended to the journal as a typed event
//! (`event_kind = "ingestion_credential"`), so it inherits the whole
//! integrity chain for free, and adds attributable, signed-at-source
//! provenance on top.
//!
//! Verification has two levels:
//! - [`verify_credential`] (attribution) — always checkable, no source
//!   needed: the credential is validly signed by the named asserter and
//!   binds to the exact fragment bytes. Trust reduces to trust in the
//!   asserter.
//! - [`verify_faithful_excerpt`] (faithful excerpt) — checkable when
//!   the source document is available: the fragment is byte-for-byte
//!   `source[locator]` and the source hashes to the claimed value.
//!
//! **Provenance is not truth.** This bridge moves the trust boundary
//! from "trust the store" to "trust the named asserter". It makes
//! origin *attributable* and (given the source) *faithful-excerpt
//! verifiable*. It does NOT establish that the source itself is correct
//! — content validity (e.g. RAG poisoning of a plausible-but-wrong
//! value) is an orthogonal axis, explicitly out of scope. A malicious
//! asserter holding a trusted key can still assert a fabricated
//! fragment with a fabricated source; the bridge makes that
//! *attributable*, and *detectable* if the real source is checkable.
//! This is the standard C2PA posture (manifest ≈ credential, assertion
//! ≈ origin claim, claim generator ≈ asserter, hard binding ≈
//! `fragment_hash`); the in-repo form is a minimal Ed25519 primitive,
//! with full C2PA manifest serialisation left to deployment (ADR-0037).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Domain-separation tag; also the prefix of the canonical encoding so
/// a credential can never be confused with another signed structure.
const INGEST_TAG: &[u8; 16] = b"hyphae-ingest-v1";

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

/// The byte range `[start, end)` within the source document that the
/// fragment was extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// Inclusive start offset.
    pub start: u64,
    /// Exclusive end offset.
    pub end: u64,
}

/// A signed assertion, made at admission time, binding a fragment's
/// bytes to a claimed origin. Self-describing: [`IngestionCredential::to_bytes`]
/// is the canonical wire/journal form, and [`IngestionCredential::signed_bytes`]
/// (everything except the signature) is exactly what the asserter signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionCredential {
    /// SHA-256 of the fragment body (binds the credential to the exact
    /// bytes).
    pub fragment_hash: [u8; 32],
    /// SHA-256 of the origin document (fixes the source).
    pub source_hash: [u8; 32],
    /// The byte range within the source the fragment came from.
    pub locator: ByteRange,
    /// The asserter's Ed25519 verifying-key bytes (who vouches).
    pub asserter: [u8; 32],
    /// Admission timestamp (asserter-supplied; seconds since epoch).
    pub asserted_at: u64,
    /// Locator of the source document (URL, DOI, path…). Not security
    /// critical (the hash fixes the source); a human/audit pointer.
    pub source_uri: String,
    /// Ed25519 signature by the asserter over [`signed_bytes`].
    ///
    /// [`signed_bytes`]: IngestionCredential::signed_bytes
    pub signature: [u8; 64],
}

impl IngestionCredential {
    /// The canonical bytes the asserter signs: everything except the
    /// signature, length-prefixed where variable, tag-prefixed.
    #[must_use]
    pub fn signed_bytes(&self) -> Vec<u8> {
        let uri = self.source_uri.as_bytes();
        let mut b = Vec::with_capacity(16 + 32 + 32 + 8 + 8 + 32 + 8 + 8 + uri.len());
        b.extend_from_slice(INGEST_TAG);
        b.extend_from_slice(&self.fragment_hash);
        b.extend_from_slice(&self.source_hash);
        b.extend_from_slice(&self.locator.start.to_le_bytes());
        b.extend_from_slice(&self.locator.end.to_le_bytes());
        b.extend_from_slice(&self.asserter);
        b.extend_from_slice(&self.asserted_at.to_le_bytes());
        b.extend_from_slice(&(uri.len() as u64).to_le_bytes());
        b.extend_from_slice(uri);
        b
    }

    /// The full canonical form (signed bytes followed by the 64-byte
    /// signature). This is what gets journaled as the credential event
    /// payload.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = self.signed_bytes();
        b.extend_from_slice(&self.signature);
        b
    }

    /// Parse a credential from its canonical form. Returns `None` on any
    /// malformed input (wrong tag, truncation, bad UTF-8 URI).
    #[must_use]
    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        let mut p = 0usize;
        let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
            let s = buf.get(*p..*p + n)?;
            *p += n;
            Some(s)
        };
        if take(&mut p, 16)? != INGEST_TAG {
            return None;
        }
        let fragment_hash: [u8; 32] = take(&mut p, 32)?.try_into().ok()?;
        let source_hash: [u8; 32] = take(&mut p, 32)?.try_into().ok()?;
        let start = u64::from_le_bytes(take(&mut p, 8)?.try_into().ok()?);
        let end = u64::from_le_bytes(take(&mut p, 8)?.try_into().ok()?);
        let asserter: [u8; 32] = take(&mut p, 32)?.try_into().ok()?;
        let asserted_at = u64::from_le_bytes(take(&mut p, 8)?.try_into().ok()?);
        let uri_len = u64::from_le_bytes(take(&mut p, 8)?.try_into().ok()?) as usize;
        let uri = take(&mut p, uri_len)?;
        let source_uri = std::str::from_utf8(uri).ok()?.to_string();
        let signature: [u8; 64] = take(&mut p, 64)?.try_into().ok()?;
        if p != buf.len() {
            return None; // trailing garbage
        }
        Some(Self {
            fragment_hash,
            source_hash,
            locator: ByteRange { start, end },
            asserter,
            asserted_at,
            source_uri,
            signature,
        })
    }
}

/// The ingestion source: holds a signing key (its own, distinct from
/// the head-anchor and witness keys) and asserts the provenance of
/// fragments it admits. In deployment this is the trusted ingester /
/// connector; here, a deterministic signer for tests and the demo.
pub struct IngestionAsserter {
    signing_key: SigningKey,
}

impl IngestionAsserter {
    /// Construct from a fixed 32-byte seed (deterministic).
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(seed),
        }
    }

    /// The asserter's public verifying key. An auditor trusts this to
    /// attribute credentials to this ingester.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Assert that `fragment` was faithfully extracted from `source` at
    /// `locator`. Returns `None` — refusing to sign — if the excerpt
    /// does not actually match (`source[locator] != fragment`), so an
    /// honest asserter cannot mint a credential for a fabricated
    /// excerpt.
    #[must_use]
    pub fn assert_ingestion(
        &self,
        fragment: &[u8],
        source: &[u8],
        source_uri: impl Into<String>,
        locator: ByteRange,
        asserted_at: u64,
    ) -> Option<IngestionCredential> {
        let slice = source.get(locator.start as usize..locator.end as usize)?;
        if slice != fragment {
            return None;
        }
        let mut cred = IngestionCredential {
            fragment_hash: sha256(fragment),
            source_hash: sha256(source),
            locator,
            asserter: self.signing_key.verifying_key().to_bytes(),
            asserted_at,
            source_uri: source_uri.into(),
            signature: [0u8; 64],
        };
        cred.signature = self.signing_key.sign(&cred.signed_bytes()).to_bytes();
        Some(cred)
    }
}

/// Attribution-level verification: the credential is validly signed by
/// `asserter_vk` and binds to the exact `fragment` bytes. Does NOT need
/// the source document. Returns `true` only when the named asserter
/// matches, the fragment hash matches, and the signature verifies.
#[must_use]
pub fn verify_credential(
    credential: &IngestionCredential,
    fragment: &[u8],
    asserter_vk: &VerifyingKey,
) -> bool {
    if credential.asserter != asserter_vk.to_bytes() {
        return false;
    }
    if credential.fragment_hash != sha256(fragment) {
        return false;
    }
    let sig = Signature::from_bytes(&credential.signature);
    asserter_vk.verify(&credential.signed_bytes(), &sig).is_ok()
}

/// Faithful-excerpt verification: everything [`verify_credential`]
/// checks, PLUS that the source hashes to the claimed value and the
/// fragment is byte-for-byte `source[locator]`. Requires the source
/// document. This is the strong check — it catches a credential whose
/// claimed origin does not actually contain the fragment.
#[must_use]
pub fn verify_faithful_excerpt(
    credential: &IngestionCredential,
    fragment: &[u8],
    source: &[u8],
    asserter_vk: &VerifyingKey,
) -> bool {
    if !verify_credential(credential, fragment, asserter_vk) {
        return false;
    }
    if credential.source_hash != sha256(source) {
        return false;
    }
    source.get(credential.locator.start as usize..credential.locator.end as usize) == Some(fragment)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSERTER_SEED: [u8; 32] = [5u8; 32];
    const SOURCE: &[u8] =
        b"The IRS standard deduction for 2026 is 15000 dollars for single filers.";

    fn credential_for(range: ByteRange) -> (IngestionCredential, Vec<u8>, IngestionAsserter) {
        let asserter = IngestionAsserter::from_seed(&ASSERTER_SEED);
        let fragment = SOURCE[range.start as usize..range.end as usize].to_vec();
        let cred = asserter
            .assert_ingestion(
                &fragment,
                SOURCE,
                "doi:irs/2026/std-deduction",
                range,
                1_750_000_000,
            )
            .unwrap();
        (cred, fragment, asserter)
    }

    #[test]
    fn attribution_and_faithful_excerpt_pass() {
        let r = ByteRange { start: 4, end: 40 };
        let (cred, fragment, asserter) = credential_for(r);
        let vk = asserter.verifying_key();
        assert!(verify_credential(&cred, &fragment, &vk));
        assert!(verify_faithful_excerpt(&cred, &fragment, SOURCE, &vk));
    }

    #[test]
    fn wrong_asserter_key_is_rejected() {
        let r = ByteRange { start: 4, end: 40 };
        let (cred, fragment, _) = credential_for(r);
        let impostor = IngestionAsserter::from_seed(&[1u8; 32]);
        assert!(!verify_credential(
            &cred,
            &fragment,
            &impostor.verifying_key()
        ));
    }

    #[test]
    fn tampered_fragment_breaks_binding() {
        let r = ByteRange { start: 4, end: 40 };
        let (cred, _fragment, asserter) = credential_for(r);
        let tampered = b"a completely different fragment body".to_vec();
        assert!(!verify_credential(
            &cred,
            &tampered,
            &asserter.verifying_key()
        ));
    }

    #[test]
    fn honest_asserter_refuses_fabricated_excerpt() {
        // The fragment is NOT what lives at the locator in the source.
        let asserter = IngestionAsserter::from_seed(&ASSERTER_SEED);
        let fabricated = b"the deduction is 50000 dollars".to_vec();
        let r = ByteRange { start: 4, end: 40 };
        assert!(
            asserter
                .assert_ingestion(&fabricated, SOURCE, "doi:x", r, 1)
                .is_none()
        );
    }

    #[test]
    fn faithful_excerpt_fails_against_wrong_source() {
        let r = ByteRange { start: 4, end: 40 };
        let (cred, fragment, asserter) = credential_for(r);
        let other_source = b"An entirely unrelated document with different text inside it here.";
        assert!(!verify_faithful_excerpt(
            &cred,
            &fragment,
            other_source,
            &asserter.verifying_key()
        ));
    }

    #[test]
    fn canonical_bytes_round_trip() {
        let r = ByteRange { start: 4, end: 40 };
        let (cred, _f, _a) = credential_for(r);
        let bytes = cred.to_bytes();
        let parsed = IngestionCredential::from_bytes(&bytes).unwrap();
        assert_eq!(cred, parsed);
        // trailing garbage rejected
        let mut bad = bytes.clone();
        bad.push(0xFF);
        assert!(IngestionCredential::from_bytes(&bad).is_none());
        // wrong tag rejected
        let mut wrongtag = bytes.clone();
        wrongtag[0] ^= 0xFF;
        assert!(IngestionCredential::from_bytes(&wrongtag).is_none());
    }

    #[test]
    fn credential_survives_journaling_round_trip() {
        // The credential rides the journal as a typed event and is still
        // attributable after reopen — integrity (chain) + attribution
        // (ingestion) compose.
        use crate::Journal;
        let dir = std::env::temp_dir().join(format!("hyphae-ingest-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = ByteRange { start: 4, end: 40 };
        let (cred, fragment, asserter) = credential_for(r);
        {
            let mut j = Journal::open(&dir).unwrap();
            j.append("audit_memory_op", fragment.clone()).unwrap();
            j.append("ingestion_credential", cred.to_bytes()).unwrap();
        }
        let j = Journal::open(&dir).unwrap();
        assert!(j.verify().is_ok());
        let stored = j.read(1).unwrap().unwrap();
        assert_eq!(stored.event_kind, "ingestion_credential");
        let parsed = IngestionCredential::from_bytes(&stored.payload).unwrap();
        assert!(verify_credential(
            &parsed,
            &fragment,
            &asserter.verifying_key()
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
