# `shot`

Wayland、X11、またはWindowsの指定したモニターをキャプチャし、クリップボードと
`Pictures/Screenshots` の両方に PNG として保存します。WindowsではWin32/GDIを使用します。
`--all` を指定すると、接続されている全モニターを横方向に連結した1枚の画像を保存します。

クリップボードへの保存には `arboard` を使用します。Wayland で動かす場合は
`wayland-data-control` 対応のコンポジターが必要です。X11 では `DISPLAY` を使い、
i3 のような X11 ウィンドウマネージャーで動作します。Windowsでは追加の表示サーバー設定は不要です。

実行後、保存したファイルのパスとクリップボードへのコピー結果を標準出力に表示します。

モニター番号を指定せずに複数モニター環境で実行すると選択画面が表示されます。
上下キーまたは `h`/`j`/`k`/`l` で移動し、Enter で選択します。Esc または `q` でキャンセルできます。

```sh
# 複数モニター時は選択画面を表示
shot

# 2 台目（モニター番号は 0 始まり）
shot --monitor 1

# 接続中の全モニター
shot --all
```

使えるオプションは `shot --help` で確認できます。

## バックエンドの選択

バックエンドはCargo featureで選択できます。通常のビルドでは全バックエンドが有効です。
特定のバックエンドだけを有効にする場合は、デフォルトfeatureを無効にします。

```sh
# Linux: X11のみ
cargo build --no-default-features --features x11

# Linux: Waylandのみ
cargo build --no-default-features --features wayland

# Windows: Win32/GDIのみ
cargo build --no-default-features --features windows
```
