use std::io::{Cursor, Read, Write, stdin, stdout};

use xh_archive::{
    decoding::Decoder, encoding::Encoder, hashing::Hasher, packing::Packer, unpacking::Unpacker,
};

use crate::options::cli::ArchiveAction;

// TODO: impl event streaming
pub fn handle(action: &ArchiveAction) -> Result<(), eyre::Error> {
    let stdin_cursor = || {
        let mut buffer = Vec::new();
        stdin().read_to_end(&mut buffer)?;
        Ok::<_, eyre::Error>(Cursor::new(buffer))
    };
    match action {
        ArchiveAction::Pack { path } => {
            let mut buffer = Vec::new();
            let mut encoder = Encoder::new(&mut buffer);

            for event in Packer::new(path.to_path_buf()).pack() {
                encoder
                    .encode(std::iter::once(event?))
                    .expect("packer should return valid events")
            }

            stdout().lock().write_all(&buffer)?;
        }
        ArchiveAction::Unpack { path } => {
            let mut cursor = stdin_cursor()?;
            let mut decoder = Decoder::new(&mut cursor);
            let events: Vec<_> = decoder.decode().collect::<Result<_, _>>()?;

            Unpacker::new(path).unpack(events)?;
        }
        ArchiveAction::Decode => {
            let mut cursor = stdin_cursor()?;
            let mut decoder = Decoder::new(&mut cursor);
            let mut stdout = stdout().lock();

            for event in decoder.decode() {
                writeln!(stdout, "{:#?}", event?)?;
            }
        }
        ArchiveAction::Hash { each_event } => {
            let mut cursor = stdin_cursor()?;
            let mut decoder = Decoder::new(&mut cursor);
            let hashes = decoder.decode().map(|result| result.map(Hasher::hash));

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
