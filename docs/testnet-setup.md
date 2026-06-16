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

### Reference Sepolia Deployment

The minimal `ToolPassportRegistry` in this repository was deployed and confirmed
on Sepolia on June 14, 2026:

- Registry: `0x2761b873fd95bb8b1faf2ccbfd385a5e656ece8c`
- Deployment transaction:
  `0x5e74ea7e5bae0a57a6c3eec4155b37be6921ab896ef2cd8a8ff7899615c1669e`
- Deployer: `0x6123DCD37ec779b2D571d674B24140889e038C05`

Read-only verification confirmed deployed bytecode is present and the target
audit Run had an initial `recordCount` of zero before attestation.

### Reference Sepolia Attestation

The first real ToolPassport attestation for this repository was approved by the
human operator and submitted through the Rust Trust Core on June 16, 2026:

- Run: `f6da603d-8504-48a9-86b1-d97ee0587174`
- Tool: `github:langchain-ai/langgraph`
- Attestation receipt:
  `e6216da7-fdea-4894-befc-41b604c2798a`
- Transaction:
  `0x60106343b951f8175efd54c85ac5374548616d252ff6f0c49e84f98d0efd85d1`
- Explorer:
  <https://sepolia.etherscan.io/tx/0x60106343b951f8175efd54c85ac5374548616d252ff6f0c49e84f98d0efd85d1>
- Chain ID: `11155111`
- Registry:
  `0x2761b873fd95bb8b1faf2ccbfd385a5e656ece8c`
- Passport hash:
  `0x585ae6c8707004e24e084cbbc95fd35ae904ad5c8f7b68b08190e742e6dbe7fa`
- Audit log hash:
  `0x94ad2d3ffa542f643fbb14093f54b67ad8a2762910b1bf76fc3caf4993c6a6c4`
- Evidence manifest hash:
  `0xccc03725673f05d102cca313a4839023260cf4d22d7b58307a9c8efa6e3cef23`
- Onchain run ID:
  `0x801f5d2e5c6d92f0cf2597df6d896c201904db138e58f6f8831d24843424590b`

The backend persisted `attestation_submitted` and `attestation_confirmed` events
and moved the Run to `success`.

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

Container deployments must include system CA certificates because Alloy uses
HTTPS RPC endpoints during preflight and submission. The repository runtime
image installs Debian `ca-certificates`.

Invalid runtime chain configuration fails closed. Error responses identify the
invalid variable name without returning its value, and the Dashboard surfaces
that public diagnostic in the human-review boundary.

## GitHub Actions Boundary

The normal CI workflow does not need or receive testnet secrets. Any future
deployment workflow must use a protected GitHub environment, restrict secret
access, and require explicit human approval before broadcasting.

## References

- [Ethereum networks and Sepolia faucets](https://ethereum.org/developers/docs/networks/)
- [Sepolia chain settings](https://chainlist.org/chain/11155111)
- [Alchemy Ethereum API quickstart](https://www.alchemy.com/docs/reference/ethereum-api-quickstart)
- [Infura setup](https://docs.metamask.io/services/get-started/infura/)
