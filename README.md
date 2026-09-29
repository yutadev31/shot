# `shot`

Wayland または X11 の指定したモニターをキャプチャし、クリップボードと
`~/Pictures/Screenshots` の両方に PNG として保存します。

クリップボードへの保存には `arboard` を使用します。Wayland で動かす場合は
`wayland-data-control` 対応のコンポジターが必要です。X11 では `DISPLAY` を使い、
i3 のような X11 ウィンドウマネージャーで動作します。

実行後、保存したファイルのパスとクリップボードへのコピー結果を標準出力に表示します。

```sh
# 1 台目（デフォルト）
shot

# 2 台目（モニター番号は 0 始まり）
shot --monitor 1
```

使えるオプションは `shot --help` で確認できます。
