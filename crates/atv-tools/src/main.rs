mod bike_probe;
use atv_core::{Result, audio, collision, elf, iso, mesh, resource::Resource, terrain, texture};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn read_range(file: &mut File, offset: u64, bytes: u32) -> Result<Vec<u8>> {
    if bytes > 128 * 1024 * 1024 {
        return Err("file exceeds 128 MiB read limit".into());
    }
    if offset
        .checked_add(u64::from(bytes))
        .ok_or("offset overflow")?
        > file.metadata().map_err(|e| e.to_string())?.len()
    {
        return Err("extent exceeds disc size".into());
    }
    let mut b = vec![0; bytes as usize];
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

fn inventory(file: &mut File) -> Result<Vec<iso::Entry>> {
    let mut root = None;
    for sector in 16..64 {
        let pvd = read_range(file, sector * iso::SECTOR, 2048)?;
        if &pvd[1..6] != b"CD001" {
            return Err("invalid volume descriptor".into());
        }
        if pvd[0] == 1 {
            root = Some(iso::root(&pvd)?);
            break;
        }
        if pvd[0] == 255 {
            break;
        }
    }
    let mut queue = vec![(String::new(), root.ok_or("no primary volume descriptor")?)];
    let mut visited = BTreeSet::new();
    let mut files = Vec::new();
    while let Some((prefix, dir)) = queue.pop() {
        if !visited.insert((dir.lba, dir.bytes)) {
            return Err("cyclic/aliased ISO directory".into());
        }
        if visited.len() > 10000 {
            return Err("too many directories".into());
        }
        let bytes = read_range(file, u64::from(dir.lba) * iso::SECTOR, dir.bytes)?;
        for mut entry in iso::directory(&bytes)? {
            let name = format!("{prefix}/{}", entry.name);
            if entry.directory {
                queue.push((name, entry));
            } else {
                entry.name = name;
                files.push(entry);
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

fn audit(path: &str) -> Result<Value> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let entries = inventory(&mut file)?;
    let mut archives = Vec::new();
    let mut audio_results = Vec::new();
    let mut packed_audio = Vec::new();
    let mut texture_results = Vec::new();
    let mut extensions = BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    let mut total_members = 0;
    let mut blocks = 0;
    for entry in &entries {
        let ext = entry.name.rsplit('.').next().unwrap_or("");
        if matches!(ext, "RES" | "LSI") {
            let b = read_range(&mut file, u64::from(entry.lba) * iso::SECTOR, entry.bytes)?;
            let result = (|| -> Result<Value> {
                let r = Resource::parse(&b)?;
                blocks += r.validate_blocks()?;
                total_members += r.members.len();
                for (index, m) in r.members.iter().enumerate() {
                    *extensions
                        .entry(m.name.rsplit('.').next().unwrap_or("").to_ascii_lowercase())
                        .or_default() += 1;
                    if m.name.to_ascii_lowercase().ends_with(".vag") {
                        let bytes = r.extract(index)?;
                        packed_audio.push(match audio::decode_vag(&bytes) {
                            Ok(a)=>json!({"archive":entry.name,"member":m.name,"status":"decoded","info":a.info}),
                            Err(e)=>json!({"archive":entry.name,"member":m.name,"status":"unsupported","reason":e}),
                        });
                    }
                    if ext == "LSI" && m.name.to_ascii_lowercase().ends_with(".pst") {
                        let bytes = r.extract(index)?;
                        texture_results.push(match texture::decode_pst(&bytes) {
                            Ok(t)=>json!({"archive":entry.name,"member":m.name,"status":"decoded","info":t.info}),
                            Err(e)=>json!({"archive":entry.name,"member":m.name,"status":"unsupported","reason":e}),
                        });
                    }
                }
                Ok(json!({"path":entry.name,"compressed":r.compressed,"members":r.members}))
            })();
            match result {
                Ok(v) => archives.push(v),
                Err(e) => failures.push(json!({"path":entry.name,"error":e})),
            }
        } else if ext == "VAG" {
            let b = read_range(&mut file, u64::from(entry.lba) * iso::SECTOR, entry.bytes)?;
            audio_results.push(match audio::decode_vag(&b) {
                Ok(a) => json!({"path":entry.name,"status":"decoded","info":a.info}),
                Err(e) => json!({"path":entry.name,"status":"unsupported","reason":e}),
            });
        }
    }
    Ok(
        json!({"disc_files":entries.len(),"archive_count":archives.len(),"member_entries":total_members,"validated_zlib_blocks":blocks,"member_extensions":extensions,"failures":failures,"archives":archives,"audio":audio_results,"packed_audio":packed_audio,"loading_textures":texture_results}),
    )
}

fn write(path: &str, bytes: &[u8]) -> Result<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(p, bytes).map_err(|e| e.to_string())
}

fn run() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("audit-iso") if a.len()==3 => {
            let report=audit(&a[1])?;write(&a[2],&serde_json::to_vec_pretty(&report).unwrap())?;
            println!("{} files; {} archives; {} member entries; {} verified zlib blocks; {} archive failures",report["disc_files"],report["archive_count"],report["member_entries"],report["validated_zlib_blocks"],report["failures"].as_array().unwrap().len());
            if !report["failures"].as_array().unwrap().is_empty() {return Err("archive audit failed; inspect report".into());}
        }
        Some("disc-extract") if a.len()==4 => {
            let mut file=File::open(&a[1]).map_err(|e|e.to_string())?;
            let entries=inventory(&mut file)?;
            let e=entries.iter().find(|e|e.name.eq_ignore_ascii_case(&a[2])).ok_or("disc file not found")?;
            write(&a[3],&read_range(&mut file,u64::from(e.lba)*iso::SECTOR,e.bytes)?)?;
        }
        Some("psr-list" | "psr-extract") if a.len() == 3 || a.len() == 4 => {
            let extract = a[0] == "psr-extract";
            if extract != (a.len() == 4) { return Err("psr-list ISO REPORT | psr-extract ISO MEMBER OUTPUT".into()); }
            let mut file = File::open(&a[1]).map_err(|e| e.to_string())?;
            let entries = inventory(&mut file)?;
            let psr = entries.iter().find(|e| e.name.eq_ignore_ascii_case("/MXVSATV.PSR"))
                .ok_or("MXVSATV.PSR is missing")?;
            let base = u64::from(psr.lba) * iso::SECTOR;
            let header = read_range(&mut file, base, 24)?;
            let size = u32::from_le_bytes(header[16..20].try_into().unwrap());
            if size > 16 * 1024 * 1024 { return Err("PSR directory exceeds limit".into()); }
            let prefix = read_range(&mut file, base, size + 24)?;
            let directory = atv_core::psr::Directory::parse(&prefix, u64::from(psr.bytes))?;
            if extract {
                let member = directory.named(&a[2])?;
                if member.bytes > 128 * 1024 * 1024 { return Err("PSR extraction exceeds limit".into()); }
                write(&a[3], &read_range(&mut file, base + member.offset(), member.bytes)?)?;
                println!("Extracted {}: {} bytes", member.name, member.bytes);
            } else {
                write(&a[2], &serde_json::to_vec_pretty(&directory).map_err(|e| e.to_string())?)?;
                println!("{} PSR records; {} directory bytes", directory.entries.len(), directory.directory_bytes);
            }
        }
        Some("list") if a.len()==2 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            println!("{}",serde_json::to_string_pretty(&Resource::parse(&b)?.members).unwrap());
        }
        Some("extract") if a.len()==4 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            write(&a[3],&Resource::parse(&b)?.extract(a[2].parse().map_err(|_|"invalid index")?)?)?;
        }
        Some("wav") if a.len()==3 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?; let audio=audio::decode_vag(&b)?;
            write(&a[2],&audio.wav())?;println!("{}",serde_json::to_string_pretty(&audio.info).unwrap());
        }
        Some("camera-presets") if a.len()==4 => {
            let bytes=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let layout=a[2].parse().map_err(|_|"invalid viewport layout")?;
            let presets=atv_core::camera::presets(&bytes,layout)?;
            write(&a[3],&serde_json::to_vec_pretty(&presets).unwrap())?;
            println!("{} original camera presets",presets.len());
        }
        Some("motion-catalog") if a.len()==3 => {
            let bytes=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let catalog=atv_core::motion_catalog::MotionCatalog::parse(&bytes)?;
            write(&a[2],&serde_json::to_vec_pretty(&catalog).unwrap())?;
            println!("{} original motion definitions",catalog.motions.len());
        }
        Some("engine-audio") if a.len()==4 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let config=atv_core::engine_audio::EngineAudioConfig::parse(&b,a[2].parse().map_err(|_|"invalid quad id")?)?;
            write(&a[3],&serde_json::to_vec_pretty(&config).unwrap())?;
            println!("engine set {}; {} original samples",config.engine_set,config.samples.iter().map(Vec::len).sum::<usize>());
        }
        Some("stream-check") if a.len()==2 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let expected=audio::decode_vag(&b)?;
            let mut stream=audio::VagStream::new(&b[..b.len().min(2016)],b.len())?;
            let mut decoded=Vec::new();
            for chunk in b[stream.info.data_offset..].chunks(32*997) { decoded.extend(stream.decode_chunk(chunk)?); }
            if decoded!=expected.samples { return Err("stream PCM differs from whole-file decode".into()); }
            println!("{} frames, {} channels: chunked PCM matches whole-file decode",expected.info.frames,expected.info.channels);
        }
        Some("mesh") if a.len()==3 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let mesh=mesh::decode_psg(&b)?;
            write(&a[2],&serde_json::to_vec_pretty(&mesh).unwrap())?;
            println!("{} nodes; {} LODs; {} finest LOD triangles; {} unparsed trailer bytes",mesh.nodes.len(),mesh.lods.len(),mesh.lods[0].groups.iter().map(|g|g.indices.len()/3).sum::<usize>(),mesh.trailing_bytes);
        }
        Some("terrain") if a.len()==3 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let terrain=terrain::decode_tr2(&b)?;
            let heights=terrain.heightfield()?;
            let bytes:Vec<u8>=heights.iter().flat_map(|height|height.to_le_bytes()).collect();
            write(&a[2],&bytes)?;
            println!("{} by {} vertices; spacing {}; {} original tiles; {} hierarchy nodes", terrain.width_cells+1,terrain.depth_cells+1,terrain.spacing,terrain.tiles.len(),terrain.hierarchy_nodes);
        }
        Some("collision") if a.len()==3 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let mesh=collision::decode_col(&b)?;
            write(&a[2],&serde_json::to_vec_pretty(&mesh).unwrap())?;
            println!("{} original vertices; {} indexed triangles", mesh.vertices.len(), mesh.triangles.len());
        }
        Some("bike-probe") if a.len()==5 || a.len()==6 => {
            let bike=fs::read(&a[1]).map_err(|e|e.to_string())?;
            let id:u16=a[2].parse().map_err(|_|"invalid motorcycle id")?;
            let map=if a[3]=="flat" {b"flat".to_vec()} else {fs::read(&a[3]).map_err(|e|e.to_string())?};
            let materials=match std::env::var("BIKE_PROBE_TOY") { Ok(path)=>Some(atv_core::materials::terrain_materials(&fs::read(path).map_err(|e|e.to_string())?)?), Err(_)=>None };
            let report=bike_probe::run(&bike,id,&map,materials,a.get(5).map(String::as_str))?;
            write(&a[4],&serde_json::to_vec(&report).unwrap())?;
            for r in report["results"].as_array().into_iter().flatten() {
                let mut s=r.clone(); s.as_object_mut().unwrap().remove("rows");
                println!("{s}");
            }
        }
        Some("elf") if a.len()==3 => {
            let b=fs::read(&a[1]).map_err(|e|e.to_string())?;let report=elf::inspect(&b)?;
            write(&a[2],&serde_json::to_vec_pretty(&report).unwrap())?;
            println!("{} direct-call target candidates; NOT verified functions",report.direct_call_candidates.len());
        }
        _ => return Err("Usage: atv-tools audit-iso ISO REPORT | disc-extract ISO /PATH OUTPUT | list RES | extract RES INDEX OUTPUT | wav VAG OUTPUT | elf ELF REPORT | collision COL REPORT | terrain TR2 F32_OUTPUT | mesh PSG REPORT".into()),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
