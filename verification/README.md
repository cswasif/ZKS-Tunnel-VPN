# ZKS Protocol Formal Verification

This directory contains formal verification models for the ZKS protocol.

## Files

- **`zks_handshake.pv`** - ProVerif model (automated verification)
- **`zks_handshake.spthy`** - Tamarin model (stronger proofs)

## Prerequisites

### ProVerif
```bash
# Ubuntu
opam install proverif

# macOS  
brew install proverif
```

### Tamarin
```bash
# Ubuntu
sudo apt install maude graphviz
# Download from: https://github.com/tamarin-prover/tamarin-prover/releases

# macOS
brew install tamarin-prover
```

## Running Verification

```bash
proverif zks_handshake.pv
```

## Expected Output

```
Query not attacker(session_secret[]) is true.
Query not attacker(initiator_identity[]) is true.
Query event(ResponderAccepted(pkI,pkR)) ==> event(InitiatorStarted(pkI,pkR)) is true.
Query inj-event(SessionEstablished(pkI,pkR,k)) ==> inj-event(InitiatorStarted(pkI,pkR)) is true.
```

## Security Properties Verified

| Property | Query | Status |
|----------|-------|--------|
| **Session Key Secrecy** | `attacker(session_secret)` | ✅ |
| **Identity Hiding** | `attacker(initiator_identity)` | ✅ |
| **Authentication** | `ResponderAccepted ==> InitiatorStarted` | ✅ |
| **Injective Agreement** | `inj-event(SessionEstablished)` | ✅ |

## Model Overview

The ProVerif model covers:

1. **X25519 Diffie-Hellman** - Classical ephemeral key exchange
2. **Kyber768 KEM** - Post-quantum key encapsulation (abstracted)
3. **Hybrid Key Derivation** - HKDF combining both secrets
4. **Wasif-Vernam Encryption** - Transport layer encryption
5. **Identity Protection** - Static keys encrypted under ephemeral DH

## Limitations

- Kyber768 is abstracted as a generic IND-CCA2 KEM
- Timing side-channels not modeled
- Entropy Tax not included (separate mechanism)

## References

- [ProVerif Manual](https://bblanche.gitlabpages.inria.fr/proverif/manual.pdf)
- [ZKS Protocol Paper](../ZKS_Protocol_Paper.md)
