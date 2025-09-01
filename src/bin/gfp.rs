use clap::{Parser, Subcommand};
use gfp::error::PakError;
use gfp::pak_reader::implements::open_paks_by_glob;
use gfp::utils::cli;
use pathdiff::diff_paths;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// 和平精英解包工具
#[derive(Parser)]
#[command(version, about, long_about)]
struct CliArgs {
    #[clap(subcommand)]
    subcommand: Command,

    /// 处理版本号为 10 的 pak，用于 ShadowTrackerExtra/Saved/ 中的大多数 pak （默认值）
    #[arg(long, group = "pak_version", default_value = "true")]
    v10: bool,

    /// 处理版本号为 7 的 pak，用于 ShadowTrackerExtra/Saved/Paks/avatarpaks/ 中的 pak
    #[arg(long, group = "pak_version")]
    v7: bool,
}

#[derive(Subcommand)]
enum Command {
    /// 显示每个 pak 的元数据
    ///
    /// 示例：
    ///
    /// ```sh
    /// cd E:\Users\****\和平精英PC模拟器(2002291)\ShadowTrackerExtra\Saved\Paks
    /// gfp info game_patch_1.32.11.13800.pak
    /// ```
    ///
    /// 输出：
    ///
    /// ```
    /// game_patch_1.32.11.13800.pak
    ///     IsEncrypted: false
    ///     Version: 10
    /// ```
    #[command(verbatim_doc_comment)]
    Info {
        #[arg(default_value = "**/*.pak")]
        file_pattern: String,
    },

    /// 列出每个 pak 中的文件
    ///
    /// 示例：
    ///
    /// ```sh
    /// gfp ls **/*.pak
    /// ```
    #[command(verbatim_doc_comment)]
    Ls {
        /// 路径模板，例如 **/*.pak
        #[arg(required = true)]
        file_pattern: String,

        /// 是否显示条目路径
        #[arg(short = 'n', long)]
        show_entry_path: bool,
    },

    /// 将每个 pak 解包到指定路径
    ///
    /// 示例：
    ///
    /// ```sh
    /// gfp unapck **/*.pak --output_dir "D:\gfp_output"
    /// ```
    #[command(verbatim_doc_comment)]
    Unpack {
        /// 路径模板
        #[arg(required = true)]
        file_pattern: String,

        /// 输出目录
        #[arg(required = true)]
        output_dir: String,

        /// 是否在终端显示条目名
        #[arg(short = 'n', long)]
        show_entry_path: bool,
    },
    /// 读取 pak 的索引信息，写入到目标目录中对应路径下
    #[command(verbatim_doc_comment)]
    Index {
        /// 文件模板
        #[arg(required = true)]
        file_pattern: String,

        /// 输出目录
        #[arg(required = true)]
        output_dir: String,

        /// 根目录，通常为游戏根目录
        #[arg(short = 'r', long, default_value = ".")]
        base_dir: String,

        /// 是否也显示在终端
        #[arg(short = 'i', long)]
        print_index: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

    let varient = if args.v7 {
        7
    } else if args.v10 {
        10
    } else {
        panic!("Never")
    };

    match args.subcommand {
        Command::Info { file_pattern } => {
            for (pak_path, mut pak) in open_paks_by_glob(&file_pattern, varient)? {
                println!("{}", pak_path.to_string_lossy());
                println!("    IsEncrypted: {}", pak.encrypted()?);
                println!("    Version: {}", pak.version()?);
            }
        }
        Command::Ls {
            file_pattern,
            show_entry_path,
        } => {
            let file_pattern = cli::prepare_file_pattern(file_pattern);

            for (pak_path, mut pak) in open_paks_by_glob(&file_pattern, varient)? {
                if show_entry_path {
                    println!("[{}]", pak_path.to_string_lossy());
                }

                for entry_id in 0..pak.entries_count()? {
                    let entry_path = pak.get_entry_path(entry_id)?;
                    println!("[{}] {}", entry_id, entry_path);
                }
            }
        }
        Command::Unpack {
            file_pattern,
            output_dir,
            show_entry_path,
        } => {
            let file_pattern = cli::prepare_file_pattern(file_pattern);
            let output_dir = PathBuf::from(output_dir);

            for (pak_path, mut pak) in open_paks_by_glob(&file_pattern, varient)? {
                println!("[{}]", pak_path.to_string_lossy());

                if let Err(e) = (|| -> Result<(), PakError> {
                    for entry_id in 0..pak.entries_count()? {
                        let entry_path = pak.get_entry_path(entry_id)?;
                        if show_entry_path {
                            println!("[{}] {}", entry_id, entry_path);
                        }

                        let output_path = output_dir.join(&entry_path);
                        if let Some(parent) = output_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let mut output_file = File::create(&output_path)?;
                        pak.extract_entry_to_file(entry_id, &mut output_file)?;
                    }
                    Ok(())
                })() {
                    eprintln!("Error unpacking {}: {}", pak_path.to_string_lossy(), e);
                }
            }
        }
        Command::Index {
            file_pattern,
            output_dir,
            base_dir,
            print_index,
        } => {
            let file_pattern = cli::prepare_file_pattern(file_pattern);
            let base_dir = PathBuf::from(base_dir);
            let output_dir = PathBuf::from(output_dir);

            for (pak_path, mut pak) in open_paks_by_glob(&file_pattern, varient)? {
                let relative_pak_path = diff_paths(&pak_path, &base_dir).unwrap();
                println!("{}", relative_pak_path.to_string_lossy());

                let output_path = output_dir.join(&relative_pak_path);
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut output_file = File::create(&output_path)?;

                if let Err(e) = (|| -> Result<(), PakError> {
                    if print_index {
                        println!("{:>12} {}", "size", "path");
                    }

                    for entry_id in 0..pak.entries_count()? {
                        let path = pak.get_entry_path(entry_id)?;

                        if print_index {
                            println!("{}", path);
                        }

                        output_file.write(format!("{}\n", path).as_bytes())?;
                    }

                    Ok(())
                })() {
                    eprintln!(
                        "Error processing index for {}: {}",
                        pak_path.to_string_lossy(),
                        e
                    );
                }
            }
        }
    }

    Ok(())
}
