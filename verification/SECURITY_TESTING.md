# Security Testing Infrastructure

This directory contains security testing tools for the ZKS protocol.

## 1. Constant-Time Verification (dudect)

Tests that cryptographic operations don't leak timing information.

### Run:
```bash
cd zks-tunnel-client
cargo bench --bench dudect_timing
```

### Coverage:
- XOR encryption/decryption
- Key mixing operations
- Entropy validation

---

## 2. Fuzz Testing

Finds edge cases and crashes in encryption/decryption.

### Setup:
```bash
cargo install cargo-fuzz
```

### Run:
```bash
cd zks-tunnel-client
cargo +nightly fuzz run encrypt_decrypt
```

### Targets:
- `encrypt_decrypt`: Round-trip encryption test
- `onion_layers`: Multi-layer onion encryption
- `frame_parsing`: Length-prefixed frame parsing

---

## 3. Entropy Quality Tests

Validates randomness of key material.

### Using `ent`:
```bash
# Generate entropy sample
dd if=/dev/urandom bs=1M count=1 > entropy_sample.bin

# Run ent
ent entropy_sample.bin

# Expected: Entropy > 7.9 bits/byte
```

### Using `dieharder`:
```bash
dieharder -a -g 201 -f entropy_sample.bin
```

---

## 4. Static Analysis

### Clippy (Rust lints):
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Miri (Memory safety):
```bash
cargo +nightly miri test
```

---

## 5. Property-Based Testing

Uses `proptest` for invariant testing.

### Example:
```rust
proptest! {
    #[test]
    fn encrypt_decrypt_roundtrip(
        plaintext in prop::collection::vec(any::<u8>(), 0..10000),
        key in prop::collection::vec(any::<u8>(), 32..33),
    ) {
        let key: [u8; 32] = key.try_into().unwrap();
        let cipher = WasifVernam::new(key);
        
        let encrypted = cipher.encrypt(&plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        
        prop_assert_eq!(plaintext, decrypted);
    }
}
```

---

## 6. CI Integration

All tests run automatically on:
- Every push to `main`
- Every pull request
- Weekly scheduled run

See `.github/workflows/security-tests.yml`
