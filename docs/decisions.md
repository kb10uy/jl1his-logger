# 設計決定メモ

2026-10-22 時点の検討結果。Wavelog に代わる自前の QSO ログ SSoT(jl1his-logger)の方針をまとめる。

## 背景

- 現在は Wavelog を交信ログの SSoT とし、API / WebUI で記録したあと ADIF をエクスポートして eQSL、コンテストのサマリーシート、QSL カード生成、POTA に流している。
- Wavelog は用途に対して機能が過剰で、WebUI にも不満がある。
- 関連プロジェクトと Wavelog への依存:
  - wavelog-tools: `GET qso?format=adif`(フィルタ付き)、`GET catalog`(subdivisions)
  - hislog: `POST qso`、`GET qso?format=adif&since_id=&page=`、`GET station`、`GET lookup?detail=basic`、`GET catalog?topic=contest`。未送信キューを持ち、重複応答(`400 validation_error` + `details.duplicate`)を送信済みとして扱う。UUID は送らない
  - speech-to-qso: `POST qso`、`GET station`、`GET qso`(ADIF、過去交信、`callsign_match=plain`)。クライアント ID を転送時に落とす
  - grayline: `GET lookup`。トークンの `wl2_` 接頭辞で v2 を選ぶ。ADIF の長さを bytes で読む
  - qso-tools: ADIF のみを入力とする。FREQ 必須。公園は `MY_POTA_REF` から読む。長さは bytes 既定、`--lenient codepoints` あり
- 使っている Wavelog API は `qso`、`station`、`lookup`、`catalog` の 4 リソースのみ。

## 構成

- 新しい独立リポジトリ(このリポジトリ)に 1 サーバー、1 SQLite、同一オリジンの SPA / PWA。
- サーバーは Rust + axum、クライアントは SvelteKit。speech-to-qso の認証(パスキー)、配信、オフライン入力を参考にする。
- SQLite は 1 つ。WAL と busy_timeout を設定し、書き込みトランザクションを短く保つ。

## 必須機能

- QSO ストア、Station Location、Logbook、User、Membership、API トークン。
- 書き込み API(作成、更新、削除)、読み出し API(フィルタと ADIF 出力)、差分取得、SSE。
- ADIF インポート(初回移行と、既存 QSO への QSL 系フィールドのマージ)。
- 確認状態(紙 QSL、eQSL、LoTW をそれぞれ別に保持)。
- 参照データ: cty.dat、周波数とバンド、モードとサブモード、subdivisions、POTA 公園。
- lookup: cty.dat によるエンティティ情報と自ログの過去交信(name / grid / QTH)。
- WebUI: ログ一覧と検索、1 件編集、Live QSO 入力、音声入力(speech-to-qso を吸収)、Station Location 編集、ADIF インポートとエクスポート、QSL 状態の一括変更。

## 落とすもの

ダッシュボード、地図、DX クラスタ、QRZ / HamQTH 連携、CAT(hislog が rigctld を持つ)、コンテストモジュール(qso-tools の loggen がある)、アワード(必要になったら ADIF から qso-tools で集計)、衛星、テーマ、組織階層や細かい権限管理。

## 決定事項

### ID と重複判定

- QSO ID はクライアント生成の UUID v7。`POST` は冪等で、同 ID の再送は既存の 1 件を返す。
- ヒューリスティックな重複判定(同コール、同バンド、同モード、時刻の近傍)は登録を拒否せず警告のみ。専用エンドポイント(入力中の候補検査)と `POST` 応答の `warnings` の両方で返し、判定処理は共有する。
- サーバー採番の整数 `seq`(挿入順)と `updated_seq`(更新順)を Logbook ごとに持つ。削除後も再利用しない。

### 変更履歴

- 追記専用の変更ログは持たない。
- 行に `updated_seq`、`revision`、`updated_by`、`updated_at`、削除の墓標、カーソルの世代番号を持つ。墓標は消さない。
- 差分取得は `updated_seq > N` で削除済みも含めて現在行を返す(upsert / delete)。途中の変更内容と過去時点の再現は提供しない。
- 更新競合は `revision` で検出する。
- 履歴が必要になったら SQLite のトリガで更新前の JSON を積む形で後付けできる。API の形は変わらない。

### 自局情報

- QSO 作成時に Station の既定値を `STATION_CALLSIGN` / `MY_*` として行に焼き込む。Station を編集しても過去の ADIF は変わらない。
- `station_location_id` は分類と検索のために別途保持する。
- 過去を直すときは対象件数を示す明示的な一括更新を使う。
- Station は削除より archive を基本にする。

### 格納形

- コアのフィールド(call、band、mode、日時、station、QSL 状態など)はカラム、残りは JSON の文字列マップ。
- カラム化したフィールドは JSON に重複して持たない。未知の標準フィールドと `APP_*` を落とさない。
- 値は ADIF の文字列のまま保持し、検索用の正規化(call の正規化、秒単位の時刻、Hz の周波数)は派生列に置く。

### FREQ と公園

- FREQ 不明の QSO を保存できる。qso-tools 側で FREQ を省略可能に改修する。代表周波数で埋めない。
- 複数公園はストアでは 1 行で `MY_POTA_REF` に複数参照を持ち、POTA 提出用 export で公園ごとに `MY_SIG_INFO` を展開して射影する。QSO を複製しない。

### ユーザーと権限

- インスタンスの役割(User に持つ): `admin` / `user`。
- Logbook の役割(Membership に持つ): `owner` / `operator`。
- User は人。パスキーでログイン。個人コールサインを持つ。
- Logbook は個人局またはクラブ局の単位で、Station Location の所有者。
- QSO は `OPERATOR` に打った人のコールを既定で入れ、`created_by` / `updated_by` に User を記録する。
- `Membership.owner` はアプリの管理権限であり、ADIF の `OWNER_CALLSIGN` とは別概念。
- admin の責務は Logbook の外側: ユーザーの作成と無効化、招待リンク、パスキー再登録、Logbook の作成と削除と owner の付け替え、他人のトークン失効、参照データ更新、初期セットアップ、バックアップ、インスタンス設定。
- owner は自分の Logbook の Station と Membership を管理できるが、User は作れない。
- admin も QSO を読むには Membership が要る。救済操作は明示的に行い理由と actor を残す。
- operator は他人が打った QSO も修正・削除できる。
- API トークンは User に紐づけ、スコープで Logbook を限定する。平文は発行時だけ見せる。
- 最初の admin は CLI のセットアップコマンドで作り、一度だけの登録 URL を発行する。

### Wavelog API v2 互換層

- パス接頭辞 `/wl2` に置く。クライアントのベース URL を `https://host/wl2` にすれば `/wl2/index.php/api/v2/...` で届く。grayline 向けに `/wl2/api/v2/lookup` も alias として出す。
- サブドメインによる Host 分岐は axum で可能だが採らない。
- 互換トークンは発行時に 1 Logbook に固定する。grayline 用には `wl2_` 接頭辞を付けて発行する。
- Station に Wavelog の旧整数 ID を対応表として持ち、既存クライアントの `station_id` 設定を変えずに済ませる。
- 合わせる範囲は手元クライアントの依存部分のみ: `station_profile_id`、`since_id` / `lastfetchedid` / `has_more`、`format=adif`、重複時の `400 validation_error` + `details.duplicate`、`lookup?detail=basic`、`catalog` の contest と subdivisions。
- 互換の `since_id` は挿入 `seq` に固定する。編集・削除の差分同期は独自 API に移したクライアントで有効にする。
- UUID を送らない旧式 POST の扱いは今は決めない。クライアント(hislog、speech-to-qso)は後で直す。

### ADIF 入出力

- 長さは codepoints を既定とし、export のオプションで bytes を選べる。qso-tools には `--lenient codepoints` を渡す。
- 読み取りは adif-reader(github.com/kb10uy/adif-reader)を使う。
- writer は adif-reader 側に足す方向。置き場所(同 crate か隣の crate か)は保留。
- 共通 crate の切り出しは行わず、当面はこのリポジトリ内に閉じる。hislog / qso-tools / grayline の置き換えは安定後に別タスクで行う。
- QSL の正本は ADIF フィールド(`QSL_SENT` / `QSL_RCVD` / `*_VIA` / `QSLSDATE` / `QSLRDATE`、`EQSL_QSL_*`、`LOTW_QSL_*`)。別の boolean 群を持たない。
- qsl_required は `QSL_SENT in {R, Q}`、受信済みは `QSL_RCVD in {Y, V}`。
- export 時に batch として対象 UUID と `revision` を記録し、外部処理後はその batch の該当行だけ更新する。

### 配信と同期

- SSE を使う。WebSocket は使わない。
- イベントの `id` は `updated_seq`。再接続は `Last-Event-ID`、ページ再読み込み後はクライアントが保存したカーソルを `?after=` で渡す。
- 差分はフィルタで絞らず Logbook 全体を流し、クライアントが現在行で表示条件を再評価する。
- 初回同期は開始時の最大 `updated_seq` を H として全件を取り、その後 `after H` を読む。二重に届く行は upsert なので無害。
- axum の `Sse` に `keep_alive` を付ける。
- バックアップ復元後はカーソルの世代番号を変えて全再同期を要求する。

### クライアントの位置づけ

- speech-to-qso: WebUI に吸収する。音声入力画面を PWA の 1 画面にし、オフラインキューは Live 入力と共通化する。
- hislog: UUID 付きの永続未送信キューを実装してから独自 API に移す。
- grayline: 当面 lookup 互換。独自 API 化の優先度は低い。
- wavelog-tools: 当面 export / subdivisions 互換。独自 CLI が揃った時点で移す。

### 参照データ

- 更新は CLI サブコマンドで行う(取得、検証、版の切り替え)。WebUI は版の表示だけ。
- 既存 QSO に記録済みの DXCC や zone は参照データの更新で自動変更しない。
- JCX は文字列のまま扱い先頭ゼロを保持する。

## 保留

- QSL merge の自動適用フィールドと曖昧一致の扱い(import 実装時)
- バックアップと復元手順を初期版にどこまで含めるか
- ADIF writer の置き場所
- crate 構成

## 実装順(案)

1. 契約と fixture: ID と再送、自局情報の確定、ADIF の日本語と長さ、QSL 値、互換レスポンス、権限表。
2. 核と CLI: Logbook / Membership / Station / QSO、setup、import の preview と apply、export、backup。
3. 読み取り互換と WebUI: station / lookup / catalog / ADIF、ログ一覧と編集、Live 入力と音声入力、SSE、QSL の一括変更。
4. 書き込みクライアントの改修: hislog の永続 UUID キュー、speech-to-qso の吸収。
5. 移行リハーサルと切り替え: Station 単位で Wavelog の ADIF を出して取り込み、qcgen / loggen / POTA 用 ADIF の差分を確認する。
