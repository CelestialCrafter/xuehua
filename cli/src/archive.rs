use std::{
    io::{Write, stdin, stdout},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use bytes::{Bytes, BytesMut};
use log::warn;
use tempfile::tempfile;
use thiserror::Error;
use xh_archive::{decoding::Decoder, encoding::Encoder, packing::Packer, unpacking::Unpacker};
use xh_reports::{Erased, Frame, IntoReport, Report, ResultReportExt};

use crate::options::cli::ArchiveAction;

#[derive(Error, Debug, IntoReport)]
enum ArchiveActionError {
    #[error("could not execute pack action")]
    #[context(path)]
    Pack { path: PathBuf },
    #[error("could not execute unpack action")]
    #[context(path)]
    Unpack { path: PathBuf },
    #[error("could not execute decode action")]
    Decode,
    #[error("could not execute hash action")]
    Hash,
}

pub fn handle(action: ArchiveAction) -> Result<(), Report<ArchiveActionError>> {
    match action {
        ArchiveAction::Pack { path } => {
            pack(&path).wrap(|| ArchiveActionError::Pack { path }.into_report())
        }
        ArchiveAction::Unpack { path } => {
            unpack(&path).wrap(|| ArchiveActionError::Unpack { path }.into_report())
        }
        ArchiveAction::Decode => decode().wrap(|| ArchiveActionError::Decode.into_report()),
        ArchiveAction::Hash => hash().wrap(|| ArchiveActionError::Hash.into_report()),
    }
}

fn mmapped_stdin() -> Result<Bytes, Report<Erased>> {
    let try_map = |fd| unsafe { memmap2::Mmap::map(fd).map(Bytes::from_owner) };

    match try_map(stdin().as_raw_fd()) {
        Ok(mmap) => Ok(mmap),
        Err(err) => {
            warn!(
                error:err = err,
                suggestion = "try redirecting a file into stdin";
                "could not mmap stdin. attempting to copy stdin to temporary file instead"
            );

            let mut file = tempfile()?;
            std::io::copy(&mut stdin().lock(), &mut file)?;
            Ok(try_map(file.as_raw_fd())?)
        }
    }
}

fn hash() -> Result<(), Report<ArchiveActionError>> {
    let mut decoder = Decoder::new();
    let mut mmap = mmapped_stdin()?;

    decoder
        .decode_iter(&mut mmap)
        .try_for_each(|result| result.map(|_| ()))?;
    println!("{}", decoder.digest());

    Ok(())
}

fn decode() -> Result<(), Report<Erased>> {
    let mut stdout = stdout().lock();
    for event in Decoder::new().decode_iter(&mut mmapped_stdin()?) {
        writeln!(stdout, "{:#?}", event?)?;
    }

    Ok(())
}

fn unpack(path: &Path) -> Result<(), Report<()>> {
    let mut unpacker = Unpacker::new(path);
    for event in Decoder::new().decode_iter(&mut mmapped_stdin()?) {
        let event = event?;
        unpacker.unpack(event)?;
    }

    Ok(())
}

fn pack(path: &Path) -> Result<(), Report<Erased>> {
    let mut encoder = Encoder::new();
    let mut buffer = BytesMut::with_capacity(8192);
    let mut stdout = stdout().lock();

    for event in Packer::new(path.to_path_buf()).pack_iter() {
        buffer.clear();
        encoder.encode(&mut buffer, event?);
        stdout.write_all(&buffer)?
    }

    Ok(())
}
