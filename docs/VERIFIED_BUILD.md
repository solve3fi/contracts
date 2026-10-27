# Solve3 DEX verified build

Program: `HnosXNWPvKSFkY6xjUpCJ9nhbvTRhdfNTfWpgp3m4GK`

The mainnet executable deployed at slot `458181137` matches a build of this
repository's `solve` library using the committed `Cargo.lock` and Solana
verifiable-build image for CLI `4.1.0`. Its `solana-verify` executable hash is
`9786ac86815ad7ad3032c76fb6f67ef33c2f57c0304aced8bae421ccf922fe44`.

From the workspace root:

```sh
solana-verify build --library-name solve
solana-verify get-executable-hash target/deploy/solve.so
solana-verify verify-from-repo -u mainnet-beta \
  --program-id HnosXNWPvKSFkY6xjUpCJ9nhbvTRhdfNTfWpgp3m4GK \
  https://github.com/solve3fi/contracts \
  --commit-hash <this-commit> \
  --library-name solve \
  --mount-path programs/solve
```

The deployed binary embeds an outdated `security.txt` source URL. Changing
that URL changes the executable and requires a new deployment and verification.
