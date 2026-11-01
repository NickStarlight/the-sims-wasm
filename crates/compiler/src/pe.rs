//! PE32 parsing and relocation validation using the object crate.
use object::endian::LittleEndian as LE;
use object::pe;
use object::read::pe::{ImageNtHeaders, ImageOptionalHeader, Import, PeFile32};

pub struct PeSection {
    pub name: String,
    pub rva: u32,
    pub virtual_size: u32,
    pub file_offset: u32,
    pub size_of_raw_data: u32,
    pub characteristics: u32,
    pub va: u32,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeImport {
    pub dll: String,
    pub name: Option<String>,
    pub ordinal: Option<u32>,
    pub hint: Option<u16>,
    pub iat_va: u32,
}

#[derive(Clone, serde::Serialize)]
pub struct PeExport {
    pub ordinal: u32,
    pub name: Option<String>,
    pub target: ExportTarget,
}

#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportTarget {
    Address { address: u32 },
    ForwardByName { dll: String, name: String },
    ForwardByOrdinal { dll: String, ordinal: u32 },
}

pub struct PeInfo {
    pub name: Option<String>,
    pub is_dll: bool,
    pub exports: Vec<PeExport>,
    pub entry_va: u32,
    pub image_base: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub sections: Vec<PeSection>,
    pub imports: Vec<PeImport>,
    /// Raw relocation-directory location (for applyRelocations when the image
    /// loads at a base other than its preferred one).
    pub reloc_rva: u32,
    pub reloc_size: u32,
}

/// Read the fields consumed by guest mapping and function discovery.
pub fn parse(bytes: &[u8]) -> Result<PeInfo, String> {
    let file = PeFile32::parse(bytes).map_err(|e| e.to_string())?;
    let headers = file.nt_headers();
    if headers.file_header().machine.get(LE) != pe::IMAGE_FILE_MACHINE_I386 {
        return Err("not an i386 executable".into());
    }
    let optional = headers.optional_header();
    let image_base = optional.image_base() as u32;
    let declared_image_size = optional.size_of_image();
    // The loader maps whole pages. Some shipped images have an unaligned
    // SizeOfImage while their final section's raw file padding fills that page.
    let size_of_image = declared_image_size
        .checked_add(0xfff)
        .ok_or("image size overflow")?
        & !0xfff;
    image_base
        .checked_add(size_of_image)
        .ok_or("image address overflow")?;
    let entry_rva = optional.address_of_entry_point();
    if entry_rva >= declared_image_size {
        return Err("entry point exceeds image size".into());
    }
    let mut sections = Vec::new();
    for section in file.section_table().iter() {
        let rva = section.virtual_address.get(LE);
        let virtual_size = section.virtual_size.get(LE);
        let size_of_raw_data = section.size_of_raw_data.get(LE);
        if rva
            .checked_add(virtual_size)
            .is_none_or(|end| end > declared_image_size)
            || rva
                .checked_add(size_of_raw_data)
                .is_none_or(|end| end > size_of_image)
        {
            return Err("section exceeds image size".into());
        }
        let name_end = section.name.iter().position(|&b| b == 0).unwrap_or(8);
        sections.push(PeSection {
            name: latin1(&section.name[..name_end]),
            rva,
            virtual_size,
            file_offset: section.pointer_to_raw_data.get(LE),
            size_of_raw_data,
            characteristics: section.characteristics.get(LE),
            va: image_base + rva,
        });
    }
    let relocation = file.data_directory(pe::IMAGE_DIRECTORY_ENTRY_BASERELOC);
    let (name, exports) = exports(&file, image_base, size_of_image)?;
    Ok(PeInfo {
        name,
        exports,
        is_dll: headers.file_header().characteristics.get(LE) & pe::IMAGE_FILE_DLL != 0,
        entry_va: image_base + entry_rva,
        image_base,
        size_of_image,
        size_of_headers: optional.size_of_headers(),
        sections,
        imports: imports(&file, image_base, size_of_image)?,
        reloc_rva: relocation.map_or(0, |d| d.virtual_address.get(LE)),
        reloc_size: relocation.map_or(0, |d| d.size.get(LE)),
    })
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

fn imports(file: &PeFile32<'_>, image_base: u32, image_size: u32) -> Result<Vec<PeImport>, String> {
    let Some(table) = file.import_table().map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };
    let mut imports = Vec::new();
    let mut descriptors = table.descriptors().map_err(|e| e.to_string())?;
    while let Some(descriptor) = descriptors.next().map_err(|e| e.to_string())? {
        let dll = latin1(
            table
                .name(descriptor.name.get(LE))
                .map_err(|e| e.to_string())?,
        );
        let mut iat_rva = descriptor.first_thunk.get(LE);
        let original = descriptor.original_first_thunk.get(LE);
        let mut thunks = table
            .thunks(if original == 0 { iat_rva } else { original })
            .map_err(|e| e.to_string())?;
        while let Some(thunk) = thunks
            .next::<pe::ImageNtHeaders32>()
            .map_err(|e| e.to_string())?
        {
            if iat_rva.checked_add(4).is_none_or(|end| end > image_size) {
                return Err("import address exceeds image size".into());
            }
            let (name, ordinal, hint) = match table
                .import::<pe::ImageNtHeaders32>(thunk)
                .map_err(|e| e.to_string())?
            {
                Import::Ordinal(ordinal) => (None, Some(u32::from(ordinal)), None),
                Import::Name(hint, name) => (Some(latin1(name)), None, Some(hint)),
            };
            imports.push(PeImport {
                dll: dll.clone(),
                name,
                ordinal,
                hint,
                iat_va: image_base + iat_rva,
            });
            iat_rva += 4;
        }
    }
    Ok(imports)
}

/// Validate every relocation before any bytes are mapped into an existing compiler.
pub fn relocations(bytes: &[u8], info: &PeInfo, base: u32) -> Result<Vec<u32>, String> {
    if base == info.image_base {
        return Ok(Vec::new());
    }
    let file = PeFile32::parse(bytes).map_err(|e| e.to_string())?;
    if file.nt_headers().file_header().characteristics.get(LE) & pe::IMAGE_FILE_RELOCS_STRIPPED != 0
    {
        return Err("cannot rebase an image with stripped relocations".into());
    }
    if info.reloc_rva == 0 || info.reloc_size == 0 {
        return Ok(Vec::new());
    }
    let Some(mut blocks) = file
        .data_directories()
        .relocation_blocks(bytes, &file.section_table())
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut addresses = Vec::new();
    while let Some(block) = blocks.next().map_err(|e| e.to_string())? {
        // object exposes wrapped entry addresses; reject page arithmetic overflow first.
        block
            .virtual_address()
            .checked_add(0xfff)
            .ok_or("relocation page overflow")?;
        for relocation in block {
            if relocation.typ == pe::IMAGE_REL_BASED_ABSOLUTE {
                continue;
            }
            if relocation.typ != pe::IMAGE_REL_BASED_HIGHLOW {
                return Err(format!("unsupported relocation type {}", relocation.typ));
            }
            let rva = relocation.virtual_address;
            if rva
                .checked_add(4)
                .is_none_or(|end| end > info.size_of_image)
            {
                return Err("relocation exceeds image size".into());
            }
            addresses.push(base.checked_add(rva).ok_or("relocation address overflow")?);
        }
    }
    Ok(addresses)
}

/// Keep forwarders explicit: their strings are not executable addresses.
fn exports(
    file: &PeFile32<'_>,
    base: u32,
    size: u32,
) -> Result<(Option<String>, Vec<PeExport>), String> {
    let Some(table) = file.export_table().map_err(|e| e.to_string())? else {
        return Ok((None, Vec::new()));
    };
    let name_rva = table.directory().name.get(LE);
    let name = if name_rva == 0 {
        None
    } else {
        Some(latin1(
            table
                .name_from_pointer(name_rva)
                .map_err(|e| e.to_string())?,
        ))
    };
    let mut names = std::collections::BTreeMap::<u32, Vec<String>>::new();
    for (pointer, index) in table.name_iter() {
        if usize::from(index) >= table.addresses().len() {
            return Err("invalid export name ordinal".into());
        }
        names.entry(u32::from(index)).or_default().push(latin1(
            table
                .name_from_pointer(pointer)
                .map_err(|e| e.to_string())?,
        ));
    }
    let mut exports = Vec::new();
    for (index, address) in table.addresses().iter().enumerate() {
        if address.get(LE) == 0 {
            continue;
        } // An unused ordinal slot.
        let ordinal = table
            .ordinal_base()
            .checked_add(index as u32)
            .ok_or("export ordinal overflow")?;
        let target = match table
            .target_by_index(index as u32)
            .map_err(|e| e.to_string())?
        {
            object::read::pe::ExportTarget::Address(rva) => {
                if rva >= size {
                    return Err("export address exceeds image size".into());
                }
                ExportTarget::Address {
                    address: base + rva,
                }
            }
            object::read::pe::ExportTarget::ForwardByName(dll, name) => {
                ExportTarget::ForwardByName {
                    dll: latin1(dll),
                    name: latin1(name),
                }
            }
            object::read::pe::ExportTarget::ForwardByOrdinal(dll, ordinal) => {
                ExportTarget::ForwardByOrdinal {
                    dll: latin1(dll),
                    ordinal,
                }
            }
        };
        // Preserve aliases: multiple names may identify the same ordinal.
        if let Some(names) = names.remove(&(index as u32)) {
            for name in names {
                exports.push(PeExport {
                    ordinal,
                    name: Some(name),
                    target: target.clone(),
                });
            }
        } else {
            exports.push(PeExport {
                ordinal,
                name: None,
                target,
            });
        }
    }
    Ok((name, exports))
}
