# Wasif Vernam: Practical Double-Key Encryption

## Overview
The **Wasif Vernam** cipher is a practical implementation of the "Double-Key Vernam" concept designed for the ZKS Protocol. It combines the speed and security of a modern stream cipher with the "True Randomness" of a distributed entropy source.

## Architecture

$$ Ciphertext = Plaintext \oplus ChaCha20(Seed) \oplus RemoteStream $$

### Layer 1: Base Stream (ChaCha20-Poly1305)
- **Algorithm**: ChaCha20 (IETF variant) with Poly1305 MAC.
- **Key**: Derived from the X25519 shared secret established during the handshake.
- **Nonce**: 12-byte nonce, incremented per message.
- **Role**: Provides military-grade confidentiality and integrity (AEAD). Ensures the connection is secure even if the remote key stream fails or is unavailable.

### Layer 2: Enhancement Stream (Remote Key)
- **Source**: `zks-vernam` worker (LavaRand).
- **Mechanism**: A background task continuously fetches chunks of random entropy.
- **Operation**: The remote entropy bytes are XORed with the ChaCha20 keystream.
- **Role**: Adds "True Randomness" to the cipher, approximating a One-Time Pad.

## Security Properties
1.  **Confidentiality**: Guaranteed by ChaCha20 (256-bit key).
2.  **Integrity**: Guaranteed by Poly1305 MAC (128-bit tag). Prevents bit-flipping attacks.
3.  **Forward Secrecy**: Ephemeral X25519 keys are generated for each session.
4.  **Resilience**: If the Remote Stream lags or fails, the system gracefully degrades to standard ChaCha20-Poly1305, which is still cryptographically secure.

## Implementation Details
- **Struct**: `WasifVernam`
- **State**:
    - `cipher`: ChaCha20Poly1305 instance.
    - `remote_buffer`: Ring buffer holding fetched entropy.
    - `nonce_counter`: u64 counter for nonce generation.
- **Optimization**: Remote keys are fetched in parallel to the data stream to prevent blocking.
