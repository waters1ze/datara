# Sparks: The Capability-Native Package Registry for Datara

**Standard:** Sparks Protocol Specification  
**Version:** 1.0.0 (Schema: 1)  
**Mascot & Brand Heritage:** Inspired by the Datara spark mascot (`scripts/make_perfect_spark_icon.py`). To install a package is to "ignite a spark" (`dpm install sparks/<name>`).  
**Registry Architecture:** Zero-service, pure-data decentralized registry distributed via Git, static JSON over HTTPS, and local Content-Addressed Stores (CAS).

---

## 1. Architectural Philosophy: Registry as Pure Data

Unlike legacy package managers reliant on fragile centralized REST APIs and server-side databases, Sparks operates entirely as static structured data:
- The central index is a Git repository (`waters1ze/sparks-index`).
- Distribution is served as static immutable files over HTTPS (GitHub Pages / Cloudflare Pages) or `file://` offline mirrors.
- If any central hosting provider ceases operation, the entire index can be cloned and deployed to any static HTTP server or local filesystem without modifying client toolchains.

---

## 2. Sparse Index Protocol & URL Endpoints

To ensure O(1) network efficiency without cloning megabytes of index data, `dpm` uses a sparse protocol:

```text
/index.json                     -> Root registry snapshot and schema version
/schema.json                    -> JSON Schema definition for package manifests
/packages/<name>.json           -> Version history and metadata for package <name>
/packages/<name>/<version>.json -> Release manifest for specific package version
```

### 2.1 Sparse Package Manifest (`packages/<name>/<version>.json`)

```json
{
  "schema": 1,
  "name": "sparks/crypto_core",
  "version": "1.0.0",
  "description": "High-performance cryptographic primitives in pure Datara",
  "author": "Datara Core Team <core@datara.dev>",
  "license": "MIT OR Apache-2.0",
  "tarball_url": "https://github.com/datara-pkg/crypto_core/releases/download/v1.0.0/package.tar",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "public_key": "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
  "signature": "3645b20469de7b3a0f37c35f2998a444bc1fd0baab5bd9dc59265c0107a6ec0409a633ba8f460bc2bcfde8c0a876a3e14fb5ff8e8cb573b98c7cfd21f8a85501",
  "capabilities": [
    "Capability<FileRead>"
  ],
  "determinism_receipt": {
    "checksum": "d41d8cd98f00b204e9800998ecf8427e",
    "compiler": "forgen 1.0.0",
    "target": "x86_64-pc-windows-msvc"
  },
  "dependencies": {}
}
```

---

## 3. Cryptographic Security & ed25519 Signatures

### 3.1 SHA-256 Artifact Integrity
Every published tarball is hashed with SHA-256. Upon downloading, `dpm` computes `sha256(tarball_bytes)` and halts immediately with exit code 1 if the actual hash differs from the sparse index declaration.

### 3.2 ed25519 Author Signature Verification
1. Package authors sign the canonical binary tarball using their private ed25519 key.
2. The author's 32-byte public key (`public_key`) and 64-byte signature (`signature`) are published in the sparse index manifest.
3. During `dpm install sparks/<name>` or `dpm verify sparks/<name>`, the client cryptographically verifies:
   $$\text{ed25519\_verify}(\text{public\_key}, \text{tarball\_bytes}, \text{signature}) == \text{valid}$$
4. If a signature is invalid or forged, `dpm` rejects installation with error:
   `[ERR] Cryptographic signature verification failed for sparks/<name>!`

---

## 4. Capability-Native Sandboxing & Sidecar Verification

Sparks is the first package registry with compile-time capability transparency:
1. **Sidecar Matching:** The package tarball must contain `capabilities.json`. The declared capabilities in `packages/<name>/<version>.json` MUST match the tarball sidecar bit-for-bit.
2. **Pre-installation Audit:** When installing, `dpm` displays the exact system privileges required by the package before extracting files.
3. **Compile-time Enforcement:** The compiler verifier checks that code invoking `/io`, `/fs`, or `/net` effects holds the required capability token.

---

## 5. Determinism Receipts

Every release carries a `determinism_receipt` recording the byte-for-byte reproducibility checksum of its test suite. Running:
```bash
dpm verify sparks/<name>
```
downloads the package, compiles it locally with identical flags, and asserts that the generated artifacts match the published receipt.

---

## 6. Offline Fallback & Multi-Host Resiliency

`dpm` prioritizes resilient operation:
1. **Primary Host:** HTTPS registry endpoint configured in `datara.toml` or `~/.datara/config.toml`.
2. **Plain HTTP Guard:** Plain `http://` registries emit a warning and require explicit confirmation unless running in local test mode (`FORGEN_ALLOW_HTTP=1`).
3. **Offline Mode:** If network connectivity is unavailable, `dpm` falls back seamlessly to the local Content-Addressed Store (`~/.forgen_cache/store/`) or `file://` repositories.
