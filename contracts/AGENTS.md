# Contracts Agent Instructions

- Use Solidity and Foundry.
- Keep `ToolPassportRegistry` minimal and store hash commitments only.
- Do not add upgradeability or mainnet configuration.
- Deployment, wallet signing, and onchain writes require explicit human
  approval.
- Never auto-retry transactions with side effects.
- Before finishing, run `scripts/check_contracts.sh`.
