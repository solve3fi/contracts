import * as anchor from "@coral-xyz/anchor";
import {
	IGNORE_CACHE,
	PDAUtil,
	SolveContext,
	SolveIx,
	toTx,
} from "@solve33/sdk";
import type { Solve } from "../target/types/solve";
import { PROGRAM_CONFIG } from "./configs/constants";

describe("Dex Config Extension", () => {
	anchor.setProvider(anchor.AnchorProvider.env());
	const provider = anchor.getProvider();
	const program = anchor.workspace.solve as anchor.Program<Solve>;
	const ctx = SolveContext.withProvider(
		provider as anchor.AnchorProvider,
		program.programId,
	);

	it("Init config extension", async () => {
		const configAccount = await ctx.fetcher.getConfig(
			PROGRAM_CONFIG,
			IGNORE_CACHE,
		);


		if (configAccount) {
			const pad = PDAUtil.getConfigExtension(
				ctx.program.programId,
				new anchor.web3.PublicKey(PROGRAM_CONFIG),
			);

			const tx = toTx(
				ctx,
				SolveIx.initializeConfigExtensionIx(ctx.program, {
					solvesConfig: new anchor.web3.PublicKey(PROGRAM_CONFIG),
					solvesConfigExtensionPda: pad,
					funder: ctx.wallet.publicKey,
					feeAuthority: configAccount.feeAuthority,
				}),
			);
			const txid = await tx.buildAndExecute();
			console.log("dex pool config deployed at txid:", txid);

			const CONFIG_EXTENSION_WALLET =
			pad.publicKey.toBase58();

			console.log("CONFIG_EXTENSION_WALLET:", CONFIG_EXTENSION_WALLET);
		}
	});
});
