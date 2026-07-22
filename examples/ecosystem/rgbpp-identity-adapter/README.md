# RGB++ identity adapter

This package adds a CellScript sidecar policy Cell around an SDK-built RGB++
transition. It binds input 1 and output 1 to one exact Rgbpp Lock deployment
and to the 36-byte lock args committed in the policy Cell. The policy identity
and all configuration fields must be preserved in its successor.

It does not validate Bitcoin headers, PoW/difficulty, confirmations, reorgs,
RGB++ commitments, Rgbpp Lock witnesses, or BTC Time Lock rules. Those remain
the responsibility of the maintained RGB++ SDK, official deployed scripts,
and the selected BTC SPV/profile.

Builder requirements:

1. use the active `RGBPlusPlus/rgbpp-sdk` network constants and builders;
2. create the policy input/output at index 0 and RGB++ input/output at index 1;
3. store the SDK-selected Rgbpp Lock `codeHash`, `hashType`, and both packed
   36-byte args in the policy Cell;
4. keep official CellDeps and witness/commitment construction unchanged;
5. test wrong deployment, args, index, commitment, witness, and confirmation
   assumptions as independent rejects.

Upstream audit pin used when this cookbook entry was added:
`RGBPlusPlus/rgbpp-sdk@ee21eb9735c1adeb277e3a02b7f6c2f6fd1d0556`.
Refresh the pin, network constants, and cross-chain fixtures together.
