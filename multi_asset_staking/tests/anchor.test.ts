
describe("multi_asset_staking", () => {
  it("Basic test to check environment", async () => {
    console.log("Running minimal test");

    // Just log the public key and a constant value to check if the environment works
    const ownerKeypair = new web3.Keypair();
    console.log("Owner public key:", ownerKeypair.publicKey.toString());

    // Perform a simple assert
    assert.strictEqual(1 + 1, 2);
  });
});
