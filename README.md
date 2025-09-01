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
