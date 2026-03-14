fn main() {
    use awrk_datex as upi_wire;
    // A reasonably complex example message:
    // - A top-level ARRAY of 6 values:
    //   0) STRING "hello"
    //   1) U64 56
    //   2) BYTES [de ad be ef]
    //   3) UNIT
    //   4) NULL
    //   5) MAP with 2 entries:
    //        10 -> ARRAY [U64 1, U64 2]
    //        42 -> STRING "life"

    let mut enc = upi_wire::codec::encode::Encoder::new();
    enc.array(6, |seq| {
        seq.string("hello")?;
        seq.u64(56)?;
        seq.bytes(&[0xde, 0xad, 0xbe, 0xef])?;
        seq.value(|enc| {
            enc.unit();
            Ok(())
        })?;
        seq.value(|enc| {
            enc.null();
            Ok(())
        })?;
        seq.value(|enc| {
            enc.map(2, |map| {
                map.entry(
                    |enc| {
                        enc.u64(10);
                        Ok(())
                    },
                    |enc| {
                        enc.array(2, |inner| {
                            inner.u64(1)?;
                            inner.u64(2)?;
                            Ok(())
                        })
                    },
                )?;
                map.entry(
                    |enc| {
                        enc.u64(42);
                        Ok(())
                    },
                    |enc| enc.string("life"),
                )?;
                Ok(())
            })
        })?;
        Ok(())
    })
    .expect("encode demo value should succeed");
    let buf = enc.into_inner();

    println!("{}", to_hex(&buf));
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, b) in bytes.iter().copied().enumerate() {
        if i != 0 {
            out.push(' ');
        }
        use core::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}
