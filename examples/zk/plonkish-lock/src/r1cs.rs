use std::{
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    path::Path,
};

use anyhow::{ensure, Context, Result};
use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem, ConstraintSystemRef};
use cellscript_zk_private_counter::CounterCircuit;

pub fn synthesize(circuit: CounterCircuit) -> Result<ConstraintSystemRef<Fr>> {
    circuit.validate()?;
    let cs = ConstraintSystem::new_ref();
    circuit.generate_constraints(cs.clone())?;
    cs.finalize();
    ensure!(cs.is_satisfied()?, "counter witness does not satisfy the circuit");
    Ok(cs)
}

type Output = BufWriter<File>;

fn write_u32(output: &mut Output, value: usize) -> Result<()> {
    Ok(output.write_all(&u32::try_from(value)?.to_le_bytes())?)
}

fn header(output: &mut Output, magic: &[u8; 4], version: u32, sections: u32) -> Result<()> {
    output.write_all(magic)?;
    output.write_all(&version.to_le_bytes())?;
    Ok(output.write_all(&sections.to_le_bytes())?)
}

fn section(output: &mut Output, id: u32, write: impl FnOnce(&mut Output) -> Result<()>) -> Result<()> {
    output.write_all(&id.to_le_bytes())?;
    let length_offset = output.stream_position()?;
    output.write_all(&0u64.to_le_bytes())?;
    write(output)?;
    let end = output.stream_position()?;
    output.seek(SeekFrom::Start(length_offset))?;
    output.write_all(&(end - length_offset - 8).to_le_bytes())?;
    output.seek(SeekFrom::Start(end))?;
    Ok(())
}

fn field_header(output: &mut Output) -> Result<()> {
    write_u32(output, 32)?;
    Ok(output.write_all(&Fr::MODULUS.to_bytes_le())?)
}

fn scalar(output: &mut Output, value: &Fr) -> Result<()> {
    for limb in value.into_bigint().0 {
        output.write_all(&limb.to_le_bytes())?;
    }
    Ok(())
}

pub fn write_r1cs(path: &Path, cs: &ConstraintSystemRef<Fr>) -> Result<()> {
    let matrices = cs.to_matrices().context("counter R1CS matrices unavailable")?;
    let wires = matrices.num_instance_variables + matrices.num_witness_variables;
    let mut output = BufWriter::new(File::create(path)?);
    header(&mut output, b"r1cs", 1, 3)?;
    section(&mut output, 1, |output| {
        field_header(output)?;
        write_u32(output, wires)?;
        write_u32(output, 0)?; // No separate public outputs.
        write_u32(output, matrices.num_instance_variables - 1)?; // Wire zero is one.
        write_u32(output, 0)?; // The witness is supplied as assignments, without Circom private input labels.
        output.write_all(&(wires as u64).to_le_bytes())?;
        write_u32(output, matrices.num_constraints)
    })?;
    section(&mut output, 2, |output| {
        for ((a, b), c) in matrices.a.iter().zip(&matrices.b).zip(&matrices.c) {
            for row in [a, b, c] {
                write_u32(output, row.len())?;
                for (coefficient, index) in row {
                    write_u32(output, *index)?;
                    scalar(output, coefficient)?;
                }
            }
        }
        Ok(())
    })?;
    section(&mut output, 3, |output| {
        for wire in 0..wires as u64 {
            output.write_all(&wire.to_le_bytes())?;
        }
        Ok(())
    })?;
    Ok(output.flush()?)
}

pub fn write_witness(path: &Path, cs: &ConstraintSystemRef<Fr>) -> Result<()> {
    let cs = cs.borrow().context("counter assignments unavailable")?;
    let mut output = BufWriter::new(File::create(path)?);
    header(&mut output, b"wtns", 2, 2)?;
    section(&mut output, 1, |output| {
        field_header(output)?;
        write_u32(output, cs.instance_assignment.len() + cs.witness_assignment.len())
    })?;
    section(&mut output, 2, |output| {
        for value in cs.instance_assignment.iter().chain(&cs.witness_assignment) {
            scalar(output, value)?;
        }
        Ok(())
    })?;
    Ok(output.flush()?)
}
