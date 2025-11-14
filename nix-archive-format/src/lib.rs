pub mod decoding;
pub mod encoding;
pub mod state;
pub(crate) mod utils;

#[cfg(test)]
mod tests {
    use std::io::Read;

    use arbitrary::Arbitrary;
    use arbtest::arbtest;
    use log::info;

    use crate::{
        decoding::Decoder, encoding::Encoder, state::Event, state::arbitrary::ArbitraryNar,
        utils::TestingLogger,
    };

    // collapses multiple chunk events so comparing equality between
    // semantically (but not technically) equivalent event streams doesn't error
    fn chunk_collapse(events: Vec<Event>) -> Vec<Event> {
        let length = events.len();
        events
            .into_iter()
            .fold(Vec::with_capacity(length), |mut acc, event| {
                if let Some(Event::RegularContentChunk(parent)) = acc.last_mut() {
                    if let Event::RegularContentChunk(chunk) = event {
                        parent.extend(chunk);
                        return acc;
                    }
                }

                acc.push(event);
                acc
            })
    }

    fn test_roundtrip_blob(contents: &[u8]) {
        info!("blob contents: {:?}", contents);

        let decoded = Decoder::new(contents)
            .collect::<Result<Vec<_>, _>>()
            .expect("decoding should not fail");
        let decoded = chunk_collapse(decoded);
        info!("decoder output: {:#?}", decoded);

        let mut encoded = Vec::new();
        Encoder::new(decoded.iter())
            .read_to_end(&mut encoded)
            .expect("decoding should not fail");
        info!("encoder output: {:?}", encoded);

        // not using assert_eq because the decoded events are logged above
        assert!(
            contents == encoded,
            "original events did not match decoded events"
        );
    }

    #[test]
    fn test_roundtrip_blob_rust_compiler() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-compiler.nar"));
    }

    #[test]
    fn test_roundtrip_blob_rust_core() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-core.nar"));
    }

    #[test]
    fn test_roundtrip_blob_rust_std() {
        test_roundtrip_blob(include_bytes!("../blobs/rust-std.nar"));
    }

    #[test]
    fn arbtest_roundtrip() {
        TestingLogger::init();

        arbtest(|u| {
            let nar = ArbitraryNar::arbitrary(u)?;
            let events = chunk_collapse(nar.0);
            info!("event stream: {:#?}", events);

            let mut encoded = Vec::new();
            Encoder::new(events.iter())
                .read_to_end(&mut encoded)
                .expect("encoding should not fail");
            info!("encoder output: {:?}", encoded);

            let decoded = Decoder::new(encoded.as_slice())
                .collect::<Result<Vec<_>, _>>()
                .expect("decoding should not fail");
            let decoded = chunk_collapse(decoded);
            info!("decoder output: {:#?}", decoded);

            // not using assert_eq because the decoded events are logged above
            assert!(
                events == decoded,
                "original events did not match decoded events"
            );

            Ok(())
        })
        .run()
    }
}
