# `shot`

Wayland の指定したモニターを PNG として標準出力に出力します。

```sh
# 1 台目（デフォルト）
shot > screenshot.png

# 2 台目（モニター番号は 0 始まり）
shot --monitor 1 > screenshot.png
```

使えるオプションは `shot --help` で確認できます。
