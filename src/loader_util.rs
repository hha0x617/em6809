use em6809::loader::LoadedImage;

pub fn coalesce_ranges(mut ranges: Vec<(u16, u16)>) -> Vec<(u16, u16)> {
    ranges.sort_by_key(|(s, _)| *s);
    let mut out: Vec<(u16, u16)> = Vec::new();
    for (s, e) in ranges.into_iter() {
        if out.is_empty() {
            out.push((s, e));
        } else {
            let (_last_s, last_e) = out.last_mut().unwrap();
            if s <= *last_e {
                // overlap or touch
                if e > *last_e {
                    *last_e = e;
                }
            } else {
                out.push((s, e));
            }
        }
    }
    out
}

pub fn log_program_info(loaded: &LoadedImage, pc: u16) {
    if loaded.loaded_ranges.is_empty() {
        println!("Program: no image loaded (PC ${:04X})", pc);
        return;
    }
    let mut loaded_bytes_sum: usize = 0;
    for (s, e) in &loaded.loaded_ranges {
        loaded_bytes_sum = loaded_bytes_sum.saturating_add(e.wrapping_sub(*s) as usize);
    }
    let spans = coalesce_ranges(loaded.loaded_ranges.clone());
    let mut span = spans.iter().find(|(s, e)| *s <= pc && pc < *e).copied();
    if span.is_none() && !spans.is_empty() {
        let min_start = spans.iter().map(|(s, _)| *s).min().unwrap();
        let max_end = spans.iter().map(|(_, e)| *e).max().unwrap();
        span = Some((min_start, max_end));
    }
    if let Some((s, e)) = span {
        let span_bytes: usize = e.wrapping_sub(s) as usize;
        println!(
            "Program: start ${:04X}, loaded {} bytes, span ${:04X}..${:04X} ({} bytes)",
            pc,
            loaded_bytes_sum,
            s,
            e.wrapping_sub(1),
            span_bytes
        );
    }
}
