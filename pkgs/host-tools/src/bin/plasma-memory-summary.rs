use std::{fs, io, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn memory_kib(meminfo: &str, name: &str) -> Result<u64> {
    let line = meminfo
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}:")))
        .ok_or_else(|| invalid("missing physical memory counter"))?;
    let mut fields = line.split_whitespace();
    let value = fields
        .next()
        .ok_or_else(|| invalid("missing memory value"))?
        .parse()?;
    if fields.next() != Some("kB") {
        return Err(invalid("unexpected memory unit").into());
    }
    Ok(value)
}

#[derive(Default, Debug, PartialEq)]
struct Swap {
    disk_total_kib: u64,
    disk_used_kib: u64,
    zram_data: u64,
    zram_ram: u64,
}

fn swap_usage(swaps: &str, sys_block: &Path) -> Result<Swap> {
    let mut usage = Swap::default();
    for line in swaps.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(invalid("invalid /proc/swaps row").into());
        }
        let total: u64 = fields[2].parse()?;
        let used: u64 = fields[3].parse()?;
        if used > total {
            return Err(invalid("swap used exceeds capacity").into());
        }
        let name = Path::new(fields[0])
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid swap device name"))?;
        if name.strip_prefix("zram").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            let stats = fs::read_to_string(sys_block.join(name).join("mm_stat"))?;
            let counters: Vec<u64> = stats
                .split_whitespace()
                .take(3)
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            if counters.len() != 3 {
                return Err(invalid("incomplete zram mm_stat").into());
            }
            usage.zram_data += counters[0];
            // mem_used_total includes allocator overhead, unlike compr_data_size.
            usage.zram_ram += counters[2];
        } else {
            usage.disk_total_kib += total;
            usage.disk_used_kib += used;
        }
    }
    Ok(usage)
}

fn summary(meminfo: &str, swaps: &str, sys_block: &Path) -> Result<String> {
    let total = memory_kib(meminfo, "MemTotal")?;
    let available = memory_kib(meminfo, "MemAvailable")?;
    let used = total
        .checked_sub(available)
        .ok_or_else(|| invalid("available RAM exceeds physical capacity"))?;
    let swap = swap_usage(swaps, sys_block)?;
    // All values are kernel-derived integers, so no string escaping is needed.
    Ok(format!(
        "{{\"ramUsed\":{},\"ramTotal\":{},\"ramAvailable\":{},\"diskUsed\":{},\"diskTotal\":{},\"zramData\":{},\"zramRam\":{}}}",
        used * 1024,
        total * 1024,
        available * 1024,
        swap.disk_used_kib * 1024,
        swap.disk_total_kib * 1024,
        swap.zram_data,
        swap.zram_ram,
    ))
}

fn run() -> Result<String> {
    summary(
        &fs::read_to_string("/proc/meminfo")?,
        &fs::read_to_string("/proc/swaps")?,
        Path::new("/sys/block"),
    )
}

fn main() {
    match run() {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("Memory counters unavailable: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "Filename Type Size Used Priority\n";

    #[test]
    fn separates_disk_capacity_from_zram_and_counts_allocator_overhead() -> Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("zram0"))?;
        fs::write(
            root.path().join("zram0/mm_stat"),
            "21474836480 5000000000 6442450944 0 0",
        )?;
        let swaps = format!(
            "{HEADER}/swap/swapfile file 16777216 1048576 -1\n/dev/zram0 partition 33554432 20971520 100\n"
        );
        let text = summary(
            "MemTotal: 33554432 kB\nMemAvailable: 8388608 kB\n",
            &swaps,
            root.path(),
        )?;
        let counters: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(counters["ramUsed"], 24_u64 << 30);
        assert_eq!(counters["ramTotal"], 32_u64 << 30);
        assert_eq!(counters["ramAvailable"], 8_u64 << 30);
        assert_eq!(counters["diskUsed"], 1_u64 << 30);
        assert_eq!(counters["diskTotal"], 16_u64 << 30);
        assert_eq!(counters["zramData"], 20_u64 << 30);
        assert_eq!(counters["zramRam"], 6_u64 << 30);
        Ok(())
    }

    #[test]
    fn handles_disabled_swap_and_rejects_missing_or_invalid_counters() -> Result<()> {
        let root = tempfile::tempdir()?;
        assert_eq!(swap_usage(HEADER, root.path())?, Swap::default());
        assert!(summary("MemTotal: 10 kB\n", HEADER, root.path()).is_err());
        assert!(summary(
            "MemTotal: 10 kB\nMemAvailable: 11 kB\n",
            HEADER,
            root.path()
        )
        .is_err());
        assert!(swap_usage(&format!("{HEADER}/swap file 1 2 -1\n"), root.path()).is_err());
        assert!(swap_usage(
            &format!("{HEADER}/dev/zram0 partition 10 1 100\n"),
            root.path()
        )
        .is_err());
        Ok(())
    }
}
