use std::{
    fs::OpenOptions,
    io::{IoSlice, IoSliceMut, Result},
    os::unix::fs::OpenOptionsExt,
};

const CHUNK_SIZE: u64 = 4096 * 256;

// `O_DIRECT` requires all reads and writes
// to be aligned to the block device's block
// size. 4096 might not be the best, or even
// a valid one, for yours!
#[repr(align(4096))]
struct Aligned([u8; CHUNK_SIZE as usize]);

fn main() -> Result<()> {
    // start the ring
    let mut ring = rio::new().expect("create uring");

    // open output file, with `O_DIRECT` set
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_DIRECT)
        .open("file")
        .expect("open file");

    // create output buffer
    let out_buf = Aligned([42; CHUNK_SIZE as usize]);
    let out_io_slice = IoSlice::new(&out_buf.0);

    // create input buffer
    let mut in_buf = Aligned([0; CHUNK_SIZE as usize]);
    let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);

    let mut completions = vec![];

    for i in 0..(10 * 1024) {
        let at = i * CHUNK_SIZE;

        // Write using `Ordering::Link`,
        // causing the next operation to wait
        // for the this operation
        // to complete before starting.
        //
        // If this operation does not
        // fully complete, the next linked
        // operation fails with `ECANCELED`.
        //
        // io_uring executes unchained
        // operations out-of-order to
        // improve performance. It interleaves
        // operations from different chains
        // to improve performance.
        let completion = ring.write_ordered(
            &file,
            &out_io_slice,
            at,
            rio::Ordering::Link,
        )?;
        completions.push(completion);

        let completion =
            ring.read(&file, &mut in_io_slice, at)?;
        completions.push(completion);
    }

    ring.submit_all()?;

    let mut canceled = 0;
    for completion in completions.into_iter() {
        match completion.wait() {
            Err(e) if e.raw_os_error() == Some(125) => {
                canceled += 1
            }
            Ok(_) => {}
            other => panic!("error: {:?}", other),
        }
    }

    println!(
        "lost {} reads due to incomplete linked writes",
        canceled
    );

    if out_buf.0[..] != in_buf.0[..] {
        eprintln!("read buffer did not properly contain expected written bytes");
    }

    Ok(())
}
