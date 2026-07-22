# Spore identity adapter

This is an executable identity guard, not a reimplementation of the Spore
contract. It requires input 0 and output 0 to carry the exact Spore Type Script
selected in the guard lock's `Script.args` (`code_hash`, `hash_type`, and the
32-byte Spore ID). The deployed Spore contract remains responsible for Spore
data, action, cluster, extension, and immutability rules.

Builder requirements:

1. obtain the selected network/version identity from the maintained Spore SDK;
2. encode `code_hash || hash_type:u64-le || spore_id` in this lock's args;
3. put the Spore input and successor output at indexes 0 and 0;
4. include the official Spore deps and witnesses unchanged;
5. test wrong code hash, hash type, ID, input index, and output index as rejects.

Upstream audit pin used when this cookbook entry was added:
`sporeprotocol/spore-contract@9a6010e43ffd09ac9e6fe7ee389cc915e3e4f999`.
Refresh the pin and fixtures before treating a newer deployment as covered.
