use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
};

const QQ_DESKTOP: &'static str = "/usr/share/applications/qq.desktop";

fn main() {
    let mut file_content = String::new();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(QQ_DESKTOP)
        .expect(&format!("cant open {}", QQ_DESKTOP));
    file.read_to_string(&mut file_content)
        .expect(&format!("cant read: {}", QQ_DESKTOP));
    let mut new_content = String::new();
    file_content.split_inclusive('\n').for_each(|line| {
        if !line.contains("=") || line.contains("--force-device-scale-factor") {
            new_content += line;
            return;
        }
        let mut splited = line.split("=");
        let Some("Exec") = splited.next() else {
            new_content += line;
            return;
        };
        new_content += "Exec=";
        new_content += splited.next().unwrap_or_default().trim_end();
        new_content += " --force-device-scale-factor=1.2\n";
    });
    file.seek(SeekFrom::Start(0)).expect("seek error");
    file.set_len(0).expect("truncat error");
    file.write(new_content.as_bytes()).expect("can't write");
}
