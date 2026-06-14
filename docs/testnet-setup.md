# Testnet Attestation Setup

The MVP targets Ethereum Sepolia. Sepolia is the recommended Ethereum testnet
for application and smart contract development.

Do not reuse a mainnet wallet. Do not commit or paste secrets into Agent chat,
logs, issues, pull requests, or GitHub Actions.

## Environment Values

### `CHAIN_ID`

Use Sepolia's chain ID:

```env
CHAIN_ID=11155111
```

### `RPC_URL`

1. Create an account with a trusted RPC provider such as Alchemy or Infura.
2. Create an Ethereum Sepolia app or enable its Sepolia endpoint.
3. Copy the HTTPS endpoint into the local `.env`.
4. Treat the URL as a secret when it embeds an API key.

Verify the endpoint from your own terminal without sharing the URL:

```bash
cast chain-id --rpc-url "$RPC_URL"
```

The expected result is `11155111`.

### `PRIVATE_KEY`

Create a dedicated testnet-only wallet. One local Foundry option is:

```bash
cast wallet new
```

Run this yourself. The command prints sensitive key material. Put only the
private key into the local `.env`, never into Git or Agent chat. Send Sepolia
ETH from a faucet to the generated address before a deployment or attestation.

For long-term use, replace the plaintext environment key with an encrypted
keystore or external signer.

### `REGISTRY_CONTRACT`

Leave this empty until `ToolPassportRegistry` has been deployed to Sepolia.
Deployment requires explicit human approval.

After approval, a human may run the deployment from `contracts/`:

```bash
forge create src/ToolPassportRegistry.sol:ToolPassportRegistry \
  --rpc-url "$RPC_URL" \
  --private-key "$PRIVATE_KEY" \
  --broadcast
```

Copy the `Deployed to` address from the successful receipt into
`REGISTRY_CONTRACT`, then verify the address and transaction on the Sepolia
explorer.

## Attestation Submission Boundary

The Dashboard records a Sepolia-specific approval first. A separate
`POST /api/runs/{run_id}/attestation` action then asks the Rust Trust Core to:

1. atomically claim the Run's single submission attempt;
2. verify the connected RPC chain is Sepolia;
3. sign and broadcast `recordPassport` through Alloy;
4. wait for a successful receipt; and
5. persist the independent immutable Attestation Receipt.

The endpoint reads `RPC_URL` and `PRIVATE_KEY` only when called. A failed or
interrupted attempt is never retried automatically; it returns to manual review.
Before approval, `GET /api/attestation/preflight` returns only public derived
readiness fields: connected chain ID, signer address and balance, Registry
address, deployed-code presence, and issues. It never returns the RPC URL or
private key and never signs or broadcasts. The approved Registry must match the
runtime `REGISTRY_CONTRACT`.

## GitHub Actions Boundary

The normal CI workflow does not need or receive testnet secrets. Any future
deployment workflow must use a protected GitHub environment, restrict secret
access, and require explicit human approval before broadcasting.

## References

- [Ethereum networks and Sepolia faucets](https://ethereum.org/developers/docs/networks/)
- [Sepolia chain settings](https://chainlist.org/chain/11155111)
- [Alchemy Ethereum API quickstart](https://www.alchemy.com/docs/reference/ethereum-api-quickstart)
- [Infura setup](https://docs.metamask.io/services/get-started/infura/)
