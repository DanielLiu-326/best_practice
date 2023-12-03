use std::{mem::MaybeUninit, path::Path, process::Command, time::Duration};
use nix::libc::{sysconf, CPU_SET, CPU_SETSIZE, CPU_ZERO, _SC_NPROCESSORS_ONLN};

const PROC_NAMES: [&'static str; 5] = [
    "gjs",
    "gjs-console",
    "gnome-shell",
    "Xwayland",
    "mutter-x11-frames",
];

pub fn execute(exe: &str, args: &[&str]) {
    let res = Command::new(exe).args(args).spawn();
    match res{
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

fn do_renice() {
    // create sets
    let cpu_count: usize = unsafe { sysconf(_SC_NPROCESSORS_ONLN) }.try_into().unwrap();
    let gnome_cpu = cpu_count / 2;
    let gnome_set = unsafe {
        let mut cpu_set = MaybeUninit::zeroed().assume_init();
        CPU_ZERO(&mut cpu_set);
        // for cpu in 0..cpu_count {
        //     CPU_SET(cpu, &mut cpu_set);
        // }
        CPU_SET(gnome_cpu, &mut cpu_set);
        cpu_set
    };
    let other_set = unsafe {
        let mut cpu_set = MaybeUninit::zeroed().assume_init();
        CPU_ZERO(&mut cpu_set);
        for cpu in 0..cpu_count {
            if cpu != gnome_cpu {
                CPU_SET(cpu, &mut cpu_set);
            }
        }
        cpu_set
    };

    // iterate all processes
    let processes = procfs::process::all_processes().unwrap();
    for process in processes {
        let process = process.unwrap();
        let Ok(exe) = process.exe() else {
            continue;
        };
        let str_pid = process.pid().to_string();
        let cpu_set_size = CPU_SETSIZE.try_into().unwrap();
        let cpu_set = if is_gnome_proc(exe.as_path()) {
            execute("renice", &["-20", "-p", &str_pid]);
            &gnome_set
        } else {
            &other_set
        };
        unsafe {
            nix::libc::sched_setaffinity(process.pid, cpu_set_size, cpu_set);
        }
    }
}
fn main() {
    loop {
        do_renice();
        std::thread::sleep(Duration::from_secs(1));
        println!("--------------")
    }
}
