use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, ParseResult};
use exiftool::ExifTool;
use rexiv2::{LogLevel, Metadata};
use std::collections::HashMap;
use std::convert::Into;
use std::io::Write;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Clone)]
struct ImageInfo {
    path: PathBuf,
    date: NaiveDateTime,
}

fn get_image_infos(images: &[PathBuf]) -> Vec<ImageInfo> {
    let shared = Arc::new(Mutex::new(Vec::<ImageInfo>::new()));
    let counter = Arc::new(std::sync::atomic::AtomicIsize::new(0));
    let pool = threadpool::ThreadPool::default();
    for path in images {
        let path = path.clone();
        let shared = shared.clone();
        let total_count_str = images.len().to_string();
        let counter = counter.clone();
        pool.execute(move || {
            let left_adjust = total_count_str.len();
            let get_idx_print = || {
                let idx = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                format!("[{:left_adjust$}/{}] ", idx + 1, total_count_str)
            };
            let date = match get_date_taken(path.as_path()) {
                Ok(date) => date,
                Err(e) => {
                    println!(
                        "{} 跳过 {}, 无法获取拍摄时间：{}",
                        get_idx_print(),
                        path.to_string_lossy(),
                        e
                    );
                    return;
                }
            };
            println!("{} 获取成功：{}", get_idx_print(), path.to_string_lossy());
            shared.lock().unwrap().push(ImageInfo {
                path: path.clone(),
                date,
            });
        })
    }
    pool.join();
    std::mem::take(shared.lock().unwrap().as_mut())
}

fn scan_photos(path: &Path) -> Vec<PathBuf> {
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

        if !matches!(
            ext.to_lowercase().as_str(),
            "jpg" | "jpeg" | "rw2" | "dng" | "mp4" | "nef"
        ) {
            continue;
        }

        println!("找到: {path:?}");
        ret.push(path.to_path_buf());
    }

    ret
}

fn filter_images(image_infos: &[ImageInfo], time_range: &Range<NaiveDateTime>) -> Vec<ImageInfo> {
    let mut ret = Vec::<ImageInfo>::new();
    let mut set = HashMap::<String, &ImageInfo>::new();
    for info in image_infos {
        let file_name = info
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if let Some(contained) = set.get(&file_name)
            && contained.date.date() == info.date.date()
        {
            // println!("重复，跳过: {}", info.path.as_path().display());
            continue;
        }
        if !time_range.contains(&info.date) {
            // println!("日期不符，跳过: {}", info.path.as_path().display());
            continue;
        }
        set.insert(file_name, info);
        ret.push(info.clone());
        // println!("将复制：{}", info.path.as_path().display());
    }
    ret
}

fn ask_if_continue(question: &str, default: bool) -> bool {
    let options = if default { "[Y/n]" } else { "[y/N]" };

    loop {
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

fn get_named_args(args: &[String]) -> HashMap<&str, &str> {
    let mut map = HashMap::<&str, &str>::new();
    let mut current_name: Option<&str> = Option::default();
    for arg in args {
        if arg.starts_with('-') {
            current_name = Some(arg.as_str());
        } else if let Some(cur) = current_name {
            map.insert(cur, arg.as_str());
            current_name = None;
        }
    }

    map
}

const DATE_FMT: &str = "%Y-%m-%d";
const DATE_TIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";

fn parse_date_or_datetime(input: &str) -> ParseResult<NaiveDateTime> {
    println!("{input}");
    Ok(NaiveDateTime::parse_from_str(input.trim(), DATE_TIME_FMT)
        .or_else(|_| {
            NaiveDate::parse_from_str(input.trim(), DATE_FMT)
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap())
        })?
        .into())
}

fn get_input_time_range(
    args: &HashMap<&str, &str>,
) -> Result<Range<NaiveDateTime>, Box<dyn Error>> {
    let far_future = DateTime::UNIX_EPOCH + Duration::days(365 * 3000);
    let time_from = args.get("--time-from");
    let time_to = args.get("--time-to");
    let time_from = time_from
        .map(|input| parse_date_or_datetime(input))
        .unwrap_or(Ok(DateTime::UNIX_EPOCH.naive_utc()))?;
    let time_to = time_to
        .map(|input| parse_date_or_datetime(input))
        .unwrap_or(Ok(far_future.naive_utc()))?;
    Ok(Range {
        start: time_from,
        end: time_to,
    })
}

fn do_import(images: &[ImageInfo], dst_path: &Path) {
    let pool = threadpool::ThreadPool::default();
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for  image in images.iter(){
        let dst_path = dst_path.to_owned();
        let total_count_str = images.len().to_string();
        let image = image.clone();
        let counter = counter.clone();
        pool.execute(move || {
            let left_adjust = total_count_str.len();
            let get_idx_str = || {
                let idx = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                format!("[{:left_adjust$}/{}] ", idx + 1, total_count_str)
            };
            let path = image.path.as_path();
            // 获取拍摄时间
            let date_time = match get_date_taken(path) {
                Ok(dt) => dt,
                Err(e) => {
                    println!(
                        "{} 跳过 {}, 无法获取拍摄时间：{}",
                        get_idx_str(),
                        path.to_string_lossy(),
                        e
                    );
                    return;
                }
            };

            // 构建目标路径
            let dest_dir = dst_path
                .join(date_time.format("%Y").to_string())
                .join(date_time.format("%Y-%m-%d").to_string());

            // 创建目标目录
            if let Err(e) = fs::create_dir_all(&dest_dir) {
                eprintln!(
                    "{} 创建目录失败 {}: {}",
                    get_idx_str(),
                    dest_dir.display(),
                    e
                );
                return;
            }

            // 处理文件名冲突
            let file_name = path.file_name().unwrap();
            let dest_path = dest_dir.join(file_name);
            if dest_path.exists() {
                println!(
                    "{} 文件已存在，跳过: {}",
                    get_idx_str(),
                    dest_path.display()
                );
                return;
            }

            // 复制文件
            if let Err(e) = fs::copy(path, &dest_path) {
                eprintln!(
                    "{} 复制失败 {} -> {}: {}",
                    get_idx_str(),
                    path.display(),
                    dest_path.display(),
                    e
                );
            } else {
                println!("{} 已整理: {}", get_idx_str(), dest_path.display());
            }
        });
    }

    pool.join();
}

const USAGE_HINT: &'static str = r#"
Usage: photo_importer <to> <from>
Options:
    [--time-from]: time from. unix epoch will filled if not given.
    [--time-to]: time to. a far future time will filled if not given.
"#;

fn main() -> Result<(), Box<dyn Error>> {
    // 让 exiv2 闭嘴。
    rexiv2::set_log_level(LogLevel::MUTE);
    // 解析输入
    let args = std::env::args().collect::<Vec<String>>();
    let named_args = get_named_args(&args);

    assert!(args.len() - named_args.len() * 2 == 3, "{}", USAGE_HINT);
    let src_path = Path::new(args[2].as_str());
    let dst_path = Path::new(args[1].as_str());

    let time_range = get_input_time_range(&named_args).unwrap();
    println!("时间范围：{:?}", time_range);
    let scanned = scan_photos(src_path);
    if scanned.is_empty() {
        println!("未找到图片。");
        return Ok(());
    }

    println!("共 {} 张，开始获取图像基本信息", scanned.len());
    let infos = get_image_infos(scanned.as_slice());
    let infos = filter_images(&infos, &time_range);

    // 打印确认消息
    let question_continue = format!("找到 {} 张照片（已过滤）, 是否要开始导入？", infos.len());
    if !ask_if_continue(question_continue.as_str(), true) {
        println!("已取消");
        return Ok(());
    }
    // 开始导出
    do_import(infos.as_slice(), dst_path);

    Ok(())
}

fn get_date_taken(path: &Path) -> Result<NaiveDateTime, Box<dyn Error>> {
    // 加载元数据
    let metadata = Metadata::new_from_path(path)?;
    let datetime_str = metadata
        .get_tag_string("Exif.Photo.DateTimeOriginal")
        .or_else(|_| metadata.get_tag_string("Exif.Photo.DateTime"))
        .or_else(|_| {
            let mut tool = ExifTool::new().unwrap();
            tool.read_tag::<String>(path, "DateTimeOriginal")
                .or_else(|_| tool.read_tag::<String>(path, "DateTime"))
        })?;
    Ok(NaiveDateTime::parse_from_str(&datetime_str, "%Y:%m:%d %H:%M:%S")?.into())
}

#[test]
fn test() {
    get_date_taken(Path::new(
        "/run/media/danny/E956-B7F2/DCIM/100NZ502/DSC_1937.JPG",
    ))
    .unwrap();
}
