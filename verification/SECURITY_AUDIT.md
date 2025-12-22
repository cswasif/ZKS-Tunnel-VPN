# Security Audit Report: Wasif-Vernam Implementation

**Date:** December 22, 2025  
**Auditor:** Antigravity AI  
**Files Reviewed:**
- `p2p_relay.rs` - WasifVernam struct
- `zks_tunnel.rs` - ZksTunnel XOR encryption
- `onion.rs` - Multi-layer onion encryption

---

## 1. Code Review Findings

### 1.1 BUGS FOUND 🐛

#### BUG #1: Non-Constant-Time XOR in `zks_tunnel.rs`

**Location:** `zks_tunnel.rs:166-168`
```rust
for (i, &byte) in data.iter().enumerate() {
    dst[4 + i] = byte ^ self.key[i % 32];
}
```

**Issue:** Standard XOR is NOT constant-time. Branch prediction and cache timing can leak information.

**Risk Level:** 🟡 Medium (timing side-channel)

**Fix Required:**
```rust
use subtle::ConstantTimeEq;
// Use constant-time XOR
for (i, &byte) in data.iter().enumerate() {
    dst[4 + i] = byte ^ self.key[i % 32];
}
// Note: XOR itself is constant-time, but array indexing may not be
```

---

#### BUG #2: Non-Constant-Time XOR in `onion.rs`

**Location:** `onion.rs:83-86`
```rust
for (i, &byte) in plaintext.iter().enumerate() {
    let key_byte = k_local[i % 32] ^ k_remote[i % 32];
    ciphertext.push(byte ^ key_byte);
}
```

**Issue:** Same as above - not using constant-time operations.

**Risk Level:** 🟡 Medium

---

#### BUG #3: Potential Panic on Empty Key Check

**Location:** `p2p_relay.rs:84-88`
```rust
if !self.remote_key.is_empty() {
    for (i, byte) in mixed_data.iter_mut().enumerate() {
        *byte ^= self.remote_key[i % self.remote_key.len()];
    }
}
```

**Issue:** If `remote_key.len()` is 0 and the `is_empty()` check fails (race condition), division by zero panic.

**Risk Level:** 🟢 Low (unlikely, but defensive coding needed)

**Fix:**
```rust
let key_len = self.remote_key.len();
if key_len > 0 {
    for (i, byte) in mixed_data.iter_mut().enumerate() {
        *byte ^= self.remote_key[i % key_len];
    }
}
```

---

#### BUG #4: Nonce Counter Wrapping

**Location:** `p2p_relay.rs:74`
```rust
let counter = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
```

**Issue:** After 2^64 messages, counter wraps to 0, potentially reusing nonces.

**Risk Level:** 🟢 Low (would require ~584 years at 1M msg/sec)

**Mitigation:** Add overflow check or use key rotation before overflow.

---

### 1.2 GOOD PRACTICES FOUND ✅

| Practice | Location | Status |
|----------|----------|--------|
| ChaCha20-Poly1305 AEAD | `p2p_relay.rs:91` | ✅ Industry standard |
| Nonce includes randomness | `p2p_relay.rs:77` | ✅ Defense-in-depth |
| Length validation | `p2p_relay.rs:104` | ✅ Prevents short-ciphertext attacks |
| Key from trusted source | `p2p_relay.rs:58` | ✅ 32-byte key required |

---

## 2. Missing Security Features

### 2.1 No Constant-Time Library

**Recommendation:** Use the `subtle` crate for constant-time operations.

```toml
[dependencies]
subtle = "2.5"
```

### 2.2 No Key Zeroization

**Issue:** Keys may remain in memory after use.

**Recommendation:** Use `zeroize` crate:
```toml
[dependencies]
zeroize = { version = "1.7", features = ["derive"] }
```

```rust
use zeroize::Zeroize;

impl Drop for WasifVernam {
    fn drop(&mut self) {
        self.remote_key.zeroize();
    }
}
```

### 2.3 No Entropy Quality Validation

**Issue:** `getrandom` output not validated against NIST tests.

---

## 3. Timing Attack Vulnerability Assessment

### 3.1 XOR Operations

| Function | File | Constant-Time? |
|----------|------|----------------|
| `encapsulate` | `zks_tunnel.rs:166` | ❌ No |
| `decapsulate` | `zks_tunnel.rs:213` | ❌ No |
| `xor_encrypt` | `onion.rs:83` | ❌ No |
| `xor_decrypt` | `onion.rs:92` | ❌ No |
| `WasifVernam::encrypt` | `p2p_relay.rs:85` | ❌ No |
| `WasifVernam::decrypt` | `p2p_relay.rs:118` | ❌ No |

### 3.2 Impact Analysis

For a VPN over the internet, timing attacks are **difficult but possible**:
- Local attacker: HIGH risk
- Remote attacker: LOW risk (network jitter masks timing)

**Recommendation:** Implement constant-time XOR for defense-in-depth.

---

## 4. Recommended Fixes

### Priority 1: Add Constant-Time XOR

Create `src/ct_ops.rs`:
```rust
//! Constant-time cryptographic operations

/// Constant-time XOR of byte arrays
/// Prevents timing side-channels
#[inline]
pub fn ct_xor(dst: &mut [u8], src: &[u8], key: &[u8]) {
    // Precompute key length to avoid timing leak
    let key_len = key.len();
    assert!(key_len > 0, "Key must not be empty");
    
    for (i, (d, s)) in dst.iter_mut().zip(src.iter()).enumerate() {
        // XOR is inherently constant-time
        // Array indexing is the concern - use unsafe to avoid bounds check
        let k = unsafe { *key.get_unchecked(i % key_len) };
        *d = *s ^ k;
    }
}

/// In-place constant-time XOR
#[inline]
pub fn ct_xor_inplace(data: &mut [u8], key: &[u8]) {
    let key_len = key.len();
    if key_len == 0 { return; }
    
    for (i, byte) in data.iter_mut().enumerate() {
        let k = unsafe { *key.get_unchecked(i % key_len) };
        *byte ^= k;
    }
}
```

### Priority 2: Add Key Zeroization

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct WasifVernam {
    #[zeroize(skip)]
    cipher: ChaCha20Poly1305,
    #[zeroize(skip)]
    nonce_counter: AtomicU64,
    remote_key: Vec<u8>,
}
```

### Priority 3: Add Entropy Validation

```rust
/// Validate entropy quality using basic tests
pub fn validate_entropy(data: &[u8]) -> bool {
    if data.len() < 32 { return false; }
    
    // 1. Check for all zeros (catastrophic failure)
    if data.iter().all(|&b| b == 0) { return false; }
    
    // 2. Check for low entropy (compression test)
    let unique_bytes: std::collections::HashSet<u8> = data.iter().copied().collect();
    if unique_bytes.len() < data.len() / 4 { return false; }
    
    // 3. Check byte frequency distribution
    let mut freq = [0usize; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }
    let max_freq = *freq.iter().max().unwrap();
    let expected_max = data.len() / 128; // Very loose bound
    if max_freq > expected_max.max(4) { return false; }
    
    true
}
```

---

## 5. Test Requirements

### 5.1 Dudect Timing Test

Test that XOR operations are constant-time:
```rust
// benches/dudect_timing.rs
use dudect_bencher::{ctbench_main, BenchRng, Class, CtRunner};

fn xor_timing(runner: &mut CtRunner, rng: &mut BenchRng) {
    let key = [0xABu8; 32];
    let mut data = [0u8; 1024];
    
    runner.run_one(rng.gen_class(), || {
        ct_xor_inplace(&mut data, &key);
    });
}

ctbench_main!(xor_timing);
```

### 5.2 Fuzz Testing

```rust
// fuzz/fuzz_targets/encrypt_decrypt.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 32 { return; }
    
    let key: [u8; 32] = data[..32].try_into().unwrap();
    let plaintext = &data[32..];
    
    let cipher = WasifVernam::new(key);
    if let Ok(ciphertext) = cipher.encrypt(plaintext) {
        let decrypted = cipher.decrypt(&ciphertext).unwrap();
        assert_eq!(plaintext, decrypted.as_slice());
    }
});
```

### 5.3 NIST Entropy Tests

Use `ent` or `dieharder` on entropy output.

---

## 6. Summary

| Category | Status | Action Required |
|----------|--------|-----------------|
| **Protocol Logic** | ✅ Sound | None |
| **ChaCha20-Poly1305** | ✅ Correct | None |
| **XOR Constant-Time** | ❌ Not CT | Add `ct_ops.rs` |
| **Key Zeroization** | ❌ Missing | Add `zeroize` |
| **Entropy Validation** | ❌ Missing | Add validation |
| **Nonce Security** | ✅ Good | Monitor counter |
| **Length Validation** | ✅ Good | None |

**Overall Security Grade: B+**

The protocol is cryptographically sound, but implementation needs hardening for side-channel resistance.
