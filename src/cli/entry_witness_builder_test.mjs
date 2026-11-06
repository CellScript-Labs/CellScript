test("ordinary action and Lock entry witnesses match native metadata encoding", { skip: entryCases.length === 0 }, () => {
  for (const entry of entryCases) {
    if (entry.native_hex === null) {
      assert.throws(() => builder.encodeEntryWitness(entry.kind, entry.name, entry.args));
      continue;
    }
    const encoded = builder.encodeEntryWitness(entry.kind, entry.name, entry.args);
    assert.equal(Buffer.from(encoded).toString("hex"), entry.native_hex);
    if (entry.kind === "lock") {
      assert.deepEqual(builder.encodeLockEntryWitness(entry.name, entry.args), encoded);
    }
    assert.throws(() => builder.encodeEntryWitness(entry.kind, entry.name, [...entry.args, true]), /extra|requires|missing/);
  }
  assert.throws(() => builder.encodeEntryWitness("lock", "__not_a_declared_entry__", []), /no declared entry/);
});
