# Solve DEX - Orca Whirlpool Fork

A Solana-based decentralized exchange (DEX) smart contract forked from Orca's Whirlpool protocol. This project implements concentrated liquidity automated market maker (CLMM) functionality with enhanced features and customizations.

## Overview

Solve DEX is a concentrated liquidity DEX built on Solana that allows users to:
- Provide liquidity in concentrated price ranges
- Swap tokens with minimal slippage
- Earn fees from trading activity
- Participate in liquidity mining programs

**Program ID**: `Hvk9udYr7cFfsXN2yTKb94A5pLM7H9YvAQtnV5JQfwvz`

## Features

- **Concentrated Liquidity**: Liquidity providers can concentrate their capital in specific price ranges
- **Multiple Fee Tiers**: Support for various fee structures (0.01%, 0.05%, 0.3%, 1%)
- **Reward Emissions**: Built-in reward distribution system for liquidity providers
- **Protocol Fees**: Configurable protocol fee collection
- **Position Management**: NFT-based position tracking with metadata support
- **Tick-based Pricing**: Efficient price discovery using tick-based system

## Architecture

### Core Components

- **SolvesConfig**: Main configuration account storing authorities and global settings
- **FeeTier**: Defines fee rates and tick spacing for different pool types
- **Solve Pool**: Individual liquidity pools for token pairs
- **Position**: NFT-based liquidity positions with concentrated ranges
- **Tick Arrays**: Price level data storage for efficient swapping

### Fee Tiers

The protocol supports multiple fee tiers with different tick spacings:

| Fee Rate | Tick Spacing | Use Case |
|----------|--------------|----------|
| 0.01% | 1 | Stable pairs |
| 0.05% | 8 | Major pairs |
| 0.3% | 64 | Standard pairs |
| 1% | 128 | Exotic pairs |

## Prerequisites

- [Rust](https://rustup.rs/) (1.78.0)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) (1.17.22)
- [Anchor Framework](https://www.anchor-lang.com/docs/installation) (0.29.0)
- [Node.js](https://nodejs.org/) (16+ recommended)
- [Yarn](https://yarnpkg.com/) or [Bun](https://bun.sh/)

## Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd contracts
```

2. Install dependencies:
```bash
yarn install
# or
bun install
```

3. Build the program:
```bash
yarn build
# or
anchor build -p solve --arch sbf
```

## Configuration

### Anchor.toml

The project is configured to deploy on multiple networks:
- **Localnet**: For development and testing
- **Devnet**: For staging and integration testing  
- **Mainnet**: For production deployment

### Environment Setup

1. Configure your Solana CLI:
```bash
solana config set --url <rpc-endpoint>
solana config set --keypair <path-to-keypair>
```

2. Update the wallet path in `Anchor.toml`:
```toml
[provider]
wallet = "/path/to/your/keypair.json"
```

## Testing

Run the test suite:
```bash
anchor test
# or
yarn test
```

### Test Structure

- `00_init_dex_config.ts` - Initialize DEX configuration
- `02_init_dex_config_extension.ts` - Setup configuration extensions
- `03_init_dex_fee_tier.ts` - Create fee tier accounts

## Deployment

### Local Development

1. Start local validator:
```bash
solana-test-validator
```

2. Deploy the program:
```bash
anchor deploy
```

### Devnet/Mainnet

1. Ensure sufficient SOL balance for deployment
2. Update cluster configuration in `Anchor.toml`
3. Deploy:
```bash
anchor deploy --provider.cluster <network>
```

## Usage

### Initialize DEX Configuration

```typescript
import { Program } from "@coral-xyz/anchor";
import { Solve } from "./target/types/solve";

// Initialize the main DEX configuration
await program.methods
  .initializeConfig(
    feeAuthority,
    collectProtocolFeesAuthority,
    rewardEmissionsSuperAuthority,
    defaultProtocolFeeRate
  )
  .accounts({
    config: configPda,
    funder: wallet.publicKey,
    systemProgram: SystemProgram.programId,
  })
  .rpc();
```

### Create Fee Tier

```typescript
// Create a new fee tier
await program.methods
  .initializeFeeTier(tickSpacing, defaultFeeRate)
  .accounts({
    config: configPda,
    feeTier: feeTierPda,
    feeAuthority: feeAuthority,
    funder: wallet.publicKey,
    systemProgram: SystemProgram.programId,
  })
  .rpc();
```

## SDK Integration

The project includes integration with the `@solve33/sdk` package for easier interaction:

```typescript
import { SolveSDK } from "@solve33/sdk";

const sdk = new SolveSDK({
  connection: new Connection(rpcUrl),
  wallet: wallet,
});
```

## Security

This project implements several security measures:
- Authority-based access control
- Overflow protection in calculations
- Input validation and sanitization
- Reentrancy protection

### Security Audits

Please ensure thorough testing and consider professional security audits before mainnet deployment.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Submit a pull request

### Code Style

- Use `prettier` for code formatting: `yarn lint:fix`
- Follow Rust naming conventions
- Add comprehensive documentation for public APIs

## License

This project is licensed under the terms specified in the LICENSE file.

## Acknowledgments

- Based on [Orca's Whirlpool Protocol](https://github.com/orca-so/whirlpools)
- Built with [Anchor Framework](https://github.com/coral-xyz/anchor)
- Deployed on [Solana](https://solana.com/)

## Support

For questions and support:
- Create an issue in the repository
- Join our community Discord
- Check the documentation

## Deployed Addresses

### Mainnet
- Program: `Hvk9udYr7cFfsXN2yTKb94A5pLM7H9YvAQtnV5JQfwvz`
- Config: `FWv7G8LcHMiNgpzn7srEaFQi44T4hbC5qtYqjD8Ay4Qx`
- Config Extension: `7XTopmgkUnSU4PrDFNBPTZB5A7Su9GSAvQqsujsVrYh3`

### Fee Tier Addresses
- 1 tick spacing: `9QaxByac14rcHnqUdNk9Aw9fUsqQJSLKPMDZAuRmvqpF`
- 8 tick spacing: `5HWE94H8tqWaq1NFVv9Ha3Lv7DJzbHwEgRrEqE4mvkbX`
- 64 tick spacing: `GckJASQXuwmofBxQWN7Q4Nu5DCzVFFVgbxaMmwDndWeA`
- 128 tick spacing: `Fg9Tr1ya6Jv3mYP8HDgzRaLhjGHpv4rrrYZS4RcwVfVQ`

---

**⚠️ Disclaimer**: This software is provided as-is. Use at your own risk. Always conduct thorough testing before deploying to mainnet.
