# shot

`shot` は、指定したモニターまたは接続中の全モニターをキャプチャする
クロスプラットフォームの Rust 製 CLI ツールです。キャプチャ結果は PNG として
`Pictures/Screenshots` に保存し、同時にシステムクリップボードへコピーします。

Linux では Wayland（`wlr-screencopy`）と X11（RandR / XGetImage）、Windows では
Win32/GDI を使用します。複数モニターを `--all` で撮影すると、モニターの配置を
反映して 1 枚の画像に連結します。

## Installation

Cargo でローカルチェックアウトからインストールできます。

```sh
cargo install --path .
```

Nix を使う場合は、開発環境に入れます。

```sh
nix develop
```

## Usage

引数なしで実行すると、モニターが複数ある場合は端末上で選択できます。上下キー
または `h` / `j` / `k` / `l` で移動し、Enter で撮影、Esc または `q` でキャンセルします。

```sh
# モニターを選択して撮影
shot

# 0 始まりの番号で指定
shot --monitor 1

# Wayland の出力名や X11/Windows のモニター名で指定
shot --monitor HDMI-A-1

# 全モニターを 1 枚に連結
shot --all

# rofi でモニターを選択
shot --rofi
```

`--monitor` は番号と名前のどちらも受け付けます。保存先は通常
`~/Pictures/Screenshots/screenshot-<timestamp>.png` です。成功すると、保存した
ファイルのパスとクリップボードへのコピー結果を標準出力に表示します。

## 設定

`~/.config/shot/config.toml` で保存先とクリップボードへのコピーを指定できます。

```toml
[output]
path_format = "${pictures_dir}/Screenshots/screenshot-${timestamp}.png"
clipboard = true
```

`path_format` を省略または空文字列にすると、既定の
`Pictures/Screenshots/screenshot-<timestamp>.png` に保存します。空文字列の場合は警告を表示します。

`path_format` では `${pictures_dir}`、`${documents_dir}`、`${downloads_dir}`、
`${home_dir}`、`${timestamp}` を指定できます。`--file` は複数指定でき、指定した
場合は `path_format` より優先されます。`--clipboard` は設定ファイルの値を上書き
します。値を省略すると `true`、無効にする場合は `--clipboard false` を指定します。

詳しいオプションは次で確認できます。

```sh
shot --help
```

## Platform requirements

Linux では、`WAYLAND_DISPLAY` が設定されていれば Wayland、そうでなければ
`DISPLAY` が設定された X11 を使用します。

- Wayland: `wlr-screencopy` 対応コンポジターと、クリップボード用の
  `wayland-data-control` 対応環境が必要です。
- X11: 動作中の X サーバーと `DISPLAY` が必要です。
- Windows: 追加の表示サーバー設定は不要です。
- `--rofi`: モニター選択時に `rofi` コマンドが必要です。

## Backend features

通常のビルドでは Wayland、X11、Windows の feature がすべて有効です。特定の
バックエンドだけを有効にする場合は、デフォルト feature を無効にします。

```sh
# Linux: X11 のみ
cargo build --no-default-features --features x11

# Linux: Wayland のみ
cargo build --no-default-features --features wayland

# Windows: Win32/GDI のみ
cargo build --no-default-features --features windows
```

## Development

変更後は次のチェックを実行してください。

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

バックエンド、クリップボード、PNG 出力の変更は、利用可能なグラフィカル環境で
実際に `shot` を実行して確認します。

## License

This project is licensed under the MIT License. See
[`LICENSE.txt`](LICENSE.txt) for the full license text.
