use std::{error::Error, fs, io, path::{Path, PathBuf}};
use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use chrono::NaiveDateTime;
use rexiv2::{LogLevel, Metadata};
use exiftool::{ExifTool, ExifToolError};
use walkdir::WalkDir;

fn scan_photos(path:&Path) -> Vec<PathBuf>{
    let mut ret = Vec::<PathBuf>::new();

    println!("开始扫描: {path:?} 中的图片...");
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // 检查支持的图片格式
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "rw2"| "dng" | "mp4") {
            continue;
        }

        println!("找到: {path:?}");
        ret.push(path.to_path_buf());
    }

    ret
}


fn ask_if_continue(question:&str, default:bool) -> bool {
    let options = if default{ "[Y/n]" } else{ "[y/N]" };

    loop{
        print!("{question}{options}:");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("无法读取输入");
        let lower_trimed = input.to_lowercase().trim().to_owned();
        if lower_trimed.is_empty() {
            return default;
        }
        if lower_trimed == "y" {
            return true;
        }
        if lower_trimed == "n" {
            return false;
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // 让 exiv2 闭嘴。
    rexiv2::set_log_level(LogLevel::MUTE);

    // 解析输入
    let args = std::env::args().collect::<Vec<String>>();
    assert!(args.len() == 3, "Usage: photo_importer <to> <from>");
    let src_path = Path::new(args[2].as_str());
    let dst_path = Path::new(args[1].as_str());

    let photos = scan_photos(src_path);
    if photos.is_empty() {
        println!("未找到图片。");
        return Ok(());
    }

    // 打印确认消息
    let question_continue = format!("找到 {} 张照片, 是否要开始导入？", photos.len());
    if !ask_if_continue(question_continue.as_str(),true) {
        println!("已取消");
        return Ok(());
    }

    // 遍历需要导入的文件。
    let total_count_str = photos.len().to_string();
    for (idx, entry) in photos.iter().enumerate()
    {
        let left_adjust = total_count_str.len();
        print!("[{idx:left_adjust$}/{}] ", total_count_str);
        let path = entry.as_path();

        // 检查支持的图片格式
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // 获取拍摄时间
        let date_time = match get_date_taken(path) {
            Ok(dt) => dt,
            Err(e) => {
                println!("跳过 {}, 无法获取拍摄时间：{}",path.to_string_lossy(), e);
                continue;
            },
        };

        // 构建目标路径
        let dest_dir =dst_path
            .join(date_time.format("%Y").to_string())
            .join(date_time.format("%Y-%m-%d").to_string());

        // 创建目标目录
        if let Err(e) = fs::create_dir_all(&dest_dir) {
            eprintln!("创建目录失败 {}: {}", dest_dir.display(), e);
            continue;
        }

        // 处理文件名冲突
        let file_name = path.file_name().unwrap();
        let dest_path = dest_dir.join(file_name);
        if dest_path.exists() {
            println!("文件已存在，跳过: {}", dest_path.display());
            continue;
        }

        // 复制文件
        if let Err(e) = fs::copy(path, &dest_path) {
            eprintln!("复制失败 {} -> {}: {}", path.display(), dest_path.display(), e);
        } else {
            println!("已整理: {}", dest_path.display());
        }
    }

    Ok(())
}

#[derive(Debug)]
struct DateNotFoundInExif;

impl Display for DateNotFoundInExif {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for DateNotFoundInExif {}

fn get_date_taken(path: &Path) -> Result<NaiveDateTime, Box<dyn Error>> {
    // 加载元数据
    let metadata = Metadata::new_from_path(path)?;

    // 读取 EXIF 标签 `DateTimeOriginal`
    let datetime_str = match metadata.get_tag_string("Exif.Photo.DateTimeOriginal") {
        Ok(s) => s,
        Err(_) => { ExifTool::new()?.read_tag(path, "DateTimeOriginal")? },
    };

    Ok(NaiveDateTime::parse_from_str(&datetime_str, "%Y:%m:%d %H:%M:%S")?)
}


#[test]
fn test(){
    get_date_taken(Path::new("/run/media/danny/1f76bc9c-56be-40e7-8bd4-5987b07c3eb4/2024/2024-06-02/PANA0611.JPG"));
}