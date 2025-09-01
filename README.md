# gfp

解包和平精英的命令行工具。

```txt
Usage: gfp.exe [OPTIONS] <COMMAND>

Commands:
  info    显示每个 pak 的元数据
  ls      列出每个 pak 中的文件
  unpack  将每个 pak 解包到指定路径
  index   读取 pak 的索引信息，写入到目标目录中对应路径下
  help    Print this message or the help of the given subcommand(s)

Options:
      --v10      处理版本号为 10 的 pak，用于 ShadowTrackerExtra/Saved/ 中的大多数 pak （默认值）
      --v7       处理版本号为 7 的 pak，用于 ShadowTrackerExtra/Saved/Paks/avatarpaks/ 中的 pak
  -h, --help     Print help (see more with '--help')
  -V, --version  Print version
```

## 安装

### 方法一：从源码编译

安装 [rust 工具链](https://www.rust-lang.org/tools/install)

克隆本仓库

```shell
clone https://github.com/benborbar/gfp.git
cd gfp
```

全局安装

```shell
cargo install --path .
```

如果安装成功，此命令将会显示帮助信息：

```shell
gfp -h
```

卸载：

```shell
cargo uninstall gfp
```

### 方法二：下载编译好的可执行文件

1. 从本仓库的[发布页面](https://github.com/Benborbar/gfp/releases)下载适合你系统的可执行文件
2. 将它放在你电脑中不会忘记的地方
3. 如果你希望该命令在系统中全局可用，可以将它的路径添加到系统的PATH环境变量

卸载：移除PATH环境变量中的相应路径，并删除可执行文件即可。

## 编译

```shell
cargo build --release --target-dir ./target
```
