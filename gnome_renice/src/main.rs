use nix::libc::{
    cpu_set_t, id_t, rand, setpriority, srand, sysconf, CPU_SET, CPU_SETSIZE, CPU_ZERO,
    PRIO_PROCESS, _SC_NPROCESSORS_ONLN,
};
use std::{
    mem::MaybeUninit,
    path::Path,
    process::Command,
    time::{Duration, SystemTime},
};

/// interval of executing set priority of processes.
const RENICE_INTERVAL:usize = 2;
/// interval of reassigning cpus.
const REASSIGN_INTERVAL: usize = 600;
/// Number of cpus for Gnome processes
const DEDICATE_CPU_NUM: usize = 2;
/// Names of gnome processes.
const PROC_NAMES: [&'static str; 5] = [
    "gjs",
    "gjs-console", 
    "gnome-shell",
    "Xwayland",
    "mutter-x11-frames",
];

pub fn assign_dedicate_cpus() -> [usize; DEDICATE_CPU_NUM] {
    let cpu_count: usize = unsafe { sysconf(_SC_NPROCESSORS_ONLN) }.try_into().unwrap();
    assert!(
        cpu_count > DEDICATE_CPU_NUM,
        "Cant allocate {} cpus for gnome!",
        DEDICATE_CPU_NUM
    );

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    unsafe { srand(timestamp as u32) };

    let mut ret = [0usize; DEDICATE_CPU_NUM];
    for i in 0..DEDICATE_CPU_NUM {
        ret[i] = unsafe { rand() % (cpu_count as i32) } as usize;
    }
    return ret;
}

pub fn execute(exe: &str, args: &[&str]) {
    let res = Command::new(exe).args(args).spawn();
    match res {
        Ok(mut child) => {
            let _ = child.wait();
        }
        Err(e) => {
            println!("running external executable {} error: {}", exe, e)
        }
    }
}

fn is_gnome_proc(exe: &Path) -> bool {
    let file_name = exe.file_name().unwrap().to_str().unwrap();
    let ret = PROC_NAMES.contains(&file_name);
    if ret {
        println!("{}", file_name);
    }
    ret
}

fn do_renice(gnome_cpu_set: cpu_set_t, other_cpu_set: cpu_set_t) {
    // iterate all processes
    let processes = procfs::process::all_processes().unwrap();
    for process in processes {
        let process = process.unwrap();
        let Ok(exe) = process.exe() else {
            continue;
        };
        let pid = process.pid();
        let cpu_set_size = CPU_SETSIZE.try_into().unwrap();
        let cpu_set = if is_gnome_proc(exe.as_path()) {
            let res = unsafe { setpriority(PRIO_PROCESS, pid as id_t, -20) };
            if res < 0 {
                println!(
                    "Failed to setpriority:{}, {:?}",
                    process.pid(),
                    process.exe()
                )
            }
            &gnome_cpu_set
        } else {
            &other_cpu_set
        };
        let result = unsafe { nix::libc::sched_setaffinity(process.pid, cpu_set_size, cpu_set) };
        if result != 0 {
            println!(
                "failed to sched_setaffinity: {}, {:?}",
                process.pid,
                process.exe()
            );
        }
    }
}

fn split_cpu_sets(dedicate_cpus: &[usize; DEDICATE_CPU_NUM]) -> (cpu_set_t, cpu_set_t) {
    let gnome_cpu_set = unsafe {
        let mut cpu_set = MaybeUninit::zeroed().assume_init();
        CPU_ZERO(&mut cpu_set);
        for cpu in dedicate_cpus {
            CPU_SET(*cpu, &mut cpu_set);
        }
        cpu_set
    };
    let other_cpu_set = unsafe {
        let cpu_count: usize = sysconf(_SC_NPROCESSORS_ONLN).try_into().unwrap();
        let mut cpu_set = MaybeUninit::zeroed().assume_init();
        CPU_ZERO(&mut cpu_set);
        for cpu in 0..cpu_count {
            if !dedicate_cpus.contains(&cpu) {
                CPU_SET(cpu, &mut cpu_set);
            }
        }
        cpu_set
    };
    return (gnome_cpu_set, other_cpu_set);
}

fn main() {
    let reassign_remain = REASSIGN_INTERVAL.div_ceil(RENICE_INTERVAL);
    loop {
        let dedicate_cpus = assign_dedicate_cpus();
        let (gnome_cpu_set, other_cpu_set) = split_cpu_sets(&dedicate_cpus);
        for _ in 0..reassign_remain {
            println!("dedicate cpus:{:?}", dedicate_cpus);
            do_renice(gnome_cpu_set, other_cpu_set);
            std::thread::sleep(Duration::from_secs(2));
            println!("--------------")
        }
    }
}
