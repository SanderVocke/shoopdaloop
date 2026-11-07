use chunk_borrow::{AudioMemory, ChunkRef, Error, QuadNode};

fn main() {
    let mut memory = AudioMemory::<4>::prepare(3, 3).unwrap();
    let a = memory
        .produce_persistent(|_, output| {
            output.fill(1.);
            Ok(())
        })
        .unwrap();
    let b = memory
        .produce_persistent(|input, output| {
            let first = input.get(a)?;
            let repeated = input.get(a)?;
            for i in 0..4 {
                output[i] = first[i] + repeated[i];
            }
            Ok(())
        })
        .unwrap();
    let transient = memory
        .produce_transient(|input, output| {
            for i in 0..4 {
                output[i] = input.get(a)?[i] + input.get(b)?[i];
            }
            Ok(())
        })
        .unwrap();
    let c = memory
        .produce_persistent(|input, output| {
            output.copy_from_slice(input.get(transient)?);
            Ok(())
        })
        .unwrap();
    let mut invoked = false;
    assert_eq!(
        memory.produce_persistent(|_, _| {
            invoked = true;
            Ok(())
        }),
        Err(Error::Full)
    );
    assert!(!invoked);
    memory.finish_quantum().unwrap();
    assert_eq!(
        memory.produce_transient(|input, _| {
            input.get(transient)?;
            Ok(())
        }),
        Err(Error::Invalid)
    );
    memory.release(b).unwrap();
    assert_eq!(memory.release(b), Err(Error::Invalid));
    let replacement = memory
        .produce_persistent(|input, output| {
            assert_eq!(input.get(b), Err(Error::Invalid));
            for i in 0..4 {
                output[i] = input.get(a)?[i] + input.get(c)?[i];
            }
            Ok(())
        })
        .unwrap();
    memory
        .produce_transient(|input, output| {
            assert_eq!(input.get(a)?, &[1.; 4]);
            assert_eq!(input.get(c)?, &[3.; 4]);
            assert_eq!(input.get(replacement)?, &[4.; 4]);
            assert_eq!(input.get(ChunkRef::Silence)?, &[0.; 4]);
            output.fill(5.);
            Ok(())
        })
        .unwrap();
    memory
        .produce_transient(|_, output| {
            output.fill(6.);
            Ok(())
        })
        .unwrap();
    memory
        .produce_transient(|_, output| {
            output.fill(7.);
            Ok(())
        })
        .unwrap();
    assert_eq!(memory.produce_transient(|_, _| Ok(())), Err(Error::Full));
    memory.finish_quantum().unwrap();
    memory.release(replacement).unwrap();
    assert_eq!(
        memory.produce_persistent(|_, output| {
            output.fill(99.);
            Err(Error::Invalid)
        }),
        Err(Error::Invalid)
    );
    memory
        .produce_persistent(|_, output| {
            output.fill(8.);
            Ok(())
        })
        .unwrap();
    let batch = memory
        .produce_transient_batch(2, |input, outputs| {
            let a = input.get(a)?;
            let c = input.get(c)?;
            let (left, right) = outputs.split_at_mut(1);
            for i in 0..4 {
                left[0][i] = a[i] + c[i];
                right[0][i] = c[i] - a[i];
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(batch.chunk(2), Err(Error::Invalid));
    memory
        .produce_transient(|input, output| {
            assert_eq!(input.get(batch.chunk(0)?)?, &[4.; 4]);
            assert_eq!(input.get(batch.chunk(1)?)?, &[2.; 4]);
            output.fill(6.);
            Ok(())
        })
        .unwrap();
    assert_eq!(core::mem::size_of::<QuadNode>(), 64);
    assert_eq!(core::mem::align_of::<QuadNode>(), 64);
    println!("Borrow probe: fan-out, both destinations, multi-output batches, slot reuse, stale handles, capacity and failure checks passed.");
}
