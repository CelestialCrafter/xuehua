use std::{fs::File, path::Path};

use memmap2::{Mmap, MmapMut};
use xh_reports::prelude::*;

use crate::Options;

fn map_result(result: zstd_safe::SafeResult) -> Result<usize, ()> {
    result.map_err(|code| Report::new(zstd_safe::get_error_name(code)))
}

fn mmap_input(input: &Path) -> Result<Mmap, std::io::Error> {
    let file = File::open(input)?;
    let map = unsafe { memmap2::Mmap::map(&file) }?;

    Ok(map)
}

fn mmap_output(output: &Path, size: usize) -> Result<MmapMut, std::io::Error> {
    let file = File::create_new(output)?;
    file.set_len(size as u64)?;
    let map = unsafe { memmap2::MmapOptions::new().len(size).map_mut(&file) }?;

    Ok(map)
}

pub fn compress(options: &Options, input: &Path, output: &Path) -> Result<(), ()> {
    let input = mmap_input(input).erased()?;

    let size = zstd_safe::compress_bound(input.len());
    let mut output = mmap_output(output, size).erased()?;

    map_result(zstd_safe::compress(
        output.as_mut(),
        input.as_ref(),
        options.zstd_level,
    ))?;

    Ok(())
}

pub fn decompress(_options: &Options, input: &Path, output: &Path) -> Result<(), ()> {
    let input = mmap_input(input).erased()?;

    let size = zstd_safe::get_frame_content_size(&input)
        .map_err(|error| Report::new(error.to_string()))?;
    let size = size.unwrap_or_else(|| {
        let capacity = 1024 * 1024 * 256;
        log::warn!(capacity = capacity; "could not determine compressed file size, falling back to fixed capacity");
        capacity
    });
    let size = size.min(usize::MAX as u64) as usize;

    let mut output = mmap_output(output, size).erased()?;

    map_result(zstd_safe::decompress(output.as_mut(), &input))?;
    Ok(())
}
