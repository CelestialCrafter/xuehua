use std::io::{Write, stdin, stdout};

use xh_archive::{
    decoding::Decoder, encoding::Encoder, hashing::Hasher, packing::Packer, unpacking::Unpacker,
};

use crate::options::cli::ArchiveAction;

// TODO: impl event streaming
pub fn handle(action: &ArchiveAction) -> Result<(), eyre::Error> {
    match action {
        ArchiveAction::Pack { path } => {
            let mut encoder = Encoder::new();
            let mut stdout = stdout().lock();

            for event in Packer::new(path.clone()).pack() {
                encoder
                    .encode_writer(&mut stdout, std::iter::once(event?))
                    .expect("should be able to encode events");
            }
        }
        ArchiveAction::Unpack { path } => {
            let mut unpacker = Unpacker::new(path);
            for event in Decoder::new().decode_reader(&mut stdin()) {
                unpacker.unpack(std::iter::once(event?))?;
            }
        }
        ArchiveAction::Decode => {
            let mut stdout = stdout().lock();
            for event in Decoder::new().decode_reader(&mut stdin()) {
                writeln!(stdout, "{:#?}", event?)?;
            }
        }
        ArchiveAction::Hash { each_event } => {
            let mut stdin = stdin();
            let mut decoder = Decoder::new();
            let hashes = decoder
                .decode_reader(&mut stdin)
                .map(|result| result.map(Hasher::hash));

            if *each_event {
                let mut stdout = stdout().lock();
                for hash in hashes {
                    writeln!(stdout, "{}", hash?)?;
                }
            } else {
                let hash = Hasher::aggregate(hashes.collect::<Result<Vec<_>, _>>()?);
                println!("{hash}");
            }
        }
    }

    Ok(())
}
