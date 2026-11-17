# KT-80 — năm luật máy của OD-K6-5 vào cổng (nhật ký)

**Ngày**: 2026-11-17 · **Task**: KT-80 · **Người ghi**: Agent 2 dòng OS (uỷ quyền Agent 1,
D2-121) · **HEAD khi mở việc**: `046cc7d3` · **Quyết định**: OD-K6-5 (D2-119 + D2-120), luật
**1, 2, 3, 7, 8**; luật 5, 6 thuộc **KT-81** (chưa mở).

## 1. Vùng ghi và thứ tự làm

Vùng ghi (D2-105a): `v2/tools/**`, `v2/bin/v2`, `v2/test/rules/**`, `v2/docs/k6-machine-rules.md`,
`v2/docs/k3b-core.md` §27, `v2/agent/{STATE,BLOCKED}.md`, `v2/agent/JOURNAL/kt80-*`.
**Không chạm**: `v2/test/parity/**`, `spec/v1-parity.toml` (KT-77a đang chạy song song), `spec/`,
`v2/kernel/**`, `v2/lib/**`, `crates/`, `viscript/`, `review/`, `docs/v1/`,
`docs/v2/ViFastOs-v2-thiet-ke.md`.

Thứ tự: (1) đọc bốn verifier cũ + rút phần chung; (2) viết `v2/tools/verify-numbers.py` (một công
cụ, không copy-paste bốn bản) + ba registry; (3) cài luật 1/2/3/7/8 + ca phá; (4) nối vào cổng;
(5) chạy cổng, đo, ghi tài liệu; (6) báo lead spawn reviewer; xong mới báo freeze.

## 2. Phát hiện khi đọc verifier cũ (phần chung + lỗ hổng)

- `kt73` quét số bằng grep **chuỗi con** sau khi gọt định danh (mạnh), `kt76` quét **token** đứng
  riêng (giữ tương thích tài liệu đã qua review) — KT-80 giữ **ngữ nghĩa token của kt76** (đã được
  reviewer K6 chấp nhận cho `K6-DOD`) và **siết** những chỗ kt76 còn lỏng: so **cả dòng** thay vì
  xâu con, log phải **ở trong repo + trong git**, `scope=` không được trỏ log ca âm, tệp tiêu chí
  sinh từ máy phải chép **toàn bộ**, ngoại lệ lịch sử **ghim sha256**.
- `kt73/74/75/76/78` đều có cùng khung: `strip_identifiers` + `ALLOW`/`FORBIDDEN` + khối
  `scope=`/`scope-red=` + biến `KT** _DOCS_ROOT` để tự phá trên bản copy ⇒ gom thành một tệp, thay
  năm bản sao.
- Lỗ hổng lặp lại (đưa vào luật 8): union haystack; cờ `heading` không reset (KT-75); miễn `x.y`
  (D2-117); fence `text` không `scope=`; số một chữ số được miễn (D2-112b); số trong heading
  (D2-118D); log `/tmp` làm nguồn (D2-109C); log ca âm làm nguồn (D2-112c); bộ đếm chưa chứng minh
  vẫn ok (D2-108B); bản trích cắt cột (D2-118B); báo cáo commit trước log; ngoại lệ rồi sửa nội dung.

## 3. Đã cài gì

| Thành phần | Tệp | Vai trò |
|---|---|---|
| Công cụ (luật 1/2/3/7/8) | `v2/tools/verify-numbers.py` | `--law N`, `--self-test`, `--root` (bản copy), in `ok - luat N` / `not ok - luat N: file:dòng: MÃ lý-do` |
| Chính sách + ngoại lệ | `v2/test/rules/numbers-policy.toml` | tài liệu đích; `[[legacy]]` ghim sha256; `[[negative_log]]` (gồm `mixed`) |
| Bộ đếm | `v2/test/rules/counters.toml` | `proven`, `negative_log`, `negative_pattern`; hai bộ đếm mẫu từ KT-78 |
| Bộ đếm — sinh evidence | `v2/test/rules/counter-evidence.sh` | đọc giá trị thật từ log KT-78, fail-closed |
| Tiêu chí | `v2/test/rules/criteria.toml` | nguồn gốc + số cột; `<!-- cot-bo: … -->` |
| Hồi quy luật 8 | `v2/test/rules/verify-script-regressions.txt` | mười ba đường phá đã từng lọt |
| Ca phá từng luật | `v2/test/rules/selfbreak.sh` | 5 luật, đối chứng dương + âm, `mktemp -d`, shasum cây thật |
| Cổng | `v2/bin/v2` | lệnh `check-rules`; `do_check` gọi bước `luật máy K6 (KT-80)` |
| Tài liệu | `v2/docs/k6-machine-rules.md` | hợp đồng đầy đủ + bằng chứng; `v2/docs/k3b-core.md` §27 |

**Quyết định thiết kế** (ghi để reviewer phản biện):

1. **Một công cụ, nhiều luật** thay vì năm script: dùng chung parser khối `scope=`, bảng `ALLOW`
   định danh, resolver log, ngưỡng. Thêm luật ở KT-81 = thêm nhánh `lawN` + mã `EN_*` + ca phá.
2. **Luật 2 bằng thứ tự commit** (đúng chữ OD-K6-5): `git log --format=%H -- <path>` gom một lệnh
   cho cả tập đường dẫn (~120 ms thay vì ~66 lệnh rời), rồi `git merge-base --is-ancestor`; quan hệ
   commit bất biến nên cache theo cặp commit ở `TMPDIR` (chỉ giảm wall, không đổi kết luận). Cây bẩn
   không ảnh hưởng: chỉ đọc HEAD. Báo cáo chưa commit ⇒ `pending`; báo cáo đã commit mà log chưa
   commit ⇒ `E2_LOG_NOT_COMMITTED`; sai thứ tự ⇒ `E2_ORDER`.
3. **Luật 3** kiểm dòng **khai** `counter=<tên>` (đúng chữ OD-K6-5 #3): registry bắt buộc;
   `proven=true` cần log ca âm khớp `negative_pattern` (khác 0); `proven=false` chỉ được nằm trong
   `scope-pending=`. Hai bộ đếm mẫu `qemu_ppid1`, `isolation_bad` lấy ca âm **thật** của KT-78
   (`kt78-guard/selftest-20261117T015708Z.log`), không dựng log giả.
4. **Luật 7** theo **cột** và theo **nguồn**: registry khai bảng gốc + số cột; tệp `criteria.txt`/
   `*-DOD.md` không khai ⇒ đỏ (fail-closed cho tệp mới). Cột bỏ phải ghi tên + lý do **trong tệp**.
5. **Luật 8**: registry `.txt` là **nguồn duy nhất** của danh sách ca phá; `--self-test` dựng repo
   mini XANH trong `mktemp`, copy cho từng ca, áp sabotage, đòi **đỏ đúng mã**; verifier bị làm yếu
   ⇒ ca xanh ⇒ self-test đỏ. `selfbreak.sh` phá **bản copy của cây thật** và đòi **đối chứng dương**
   trước khi phá (tránh "đỏ vô tội vạ").
6. **Tài liệu đích = tệp đã có trong git index**: tệp chưa `git add` là bản nháp của task song song
   (D2-105a), không làm đỏ cổng của KT-80; commit xong nó vào index/HEAD và lần cổng kế kiểm nó.

## 4. Ngoại lệ (có tên + lý do + ngày, ghim sha256)

Hai mươi tệp **lịch sử** (viết trước OD-K6-5 và **ngoài vùng ghi** của KT-80) nằm trong
`[[legacy]]`: mười một báo cáo (`A1-decision-log`, `BRIEFING-Agent0-K6`, `K1C/K2/K3/K35/K3B-DOD`,
`KT-04d-goi-duyet`, `KT32-L1-MIXED`, `QUYET-DINH-A0-K6-2026-11-17`, `R1-v2`) và chín tài liệu
(`k6-capalloc`, `k6-console`, `k6-harness`, `k6-idl`, `k6-memsrv`, `k6-repro`, `k6-vfs-journal`,
`k6-vfs-pager`, `k6-virtio-blk`). Lý do chung ghi trong từng mục: số của các tệp này đã được
verifier **riêng** của task tương ứng (kt69..kt79) kiểm tại thời điểm viết, và KT-80 **không được**
sửa chúng; ghim sha để không ai thêm số mà không bị thấy. **Không** có ngoại lệ cho tài liệu mới.

Kiểm bằng log (không viết tay số):

```text scope=kt80-check-rules.log
# verify-numbers: root=/Users/mps/dvchd/ViFastOs docs=24 legacy=20 laws=1,2,3,7,8
# verify-numbers: ok=5 fail=0 wall_ms=7143 law_wall_ms={1:67,2:480,3:197,7:67,8:6233}
```

## 5. Ca phá (ĐỎ thật) và self-test

```text scope=kt80-selfbreak.log
ok 1 - luat 1: số ngoài khối scope ⇒ ĐỎ (E1_NUMBER_OUTSIDE)
ok 2 - luat 2: báo cáo commit trước log ⇒ ĐỎ (E2_ORDER)
ok 3 - luat 3: bộ đếm proven=false nằm trong khối ok ⇒ ĐỎ (E3_UNPROVEN_IN_OK)
ok 4 - luat 7: bản trích bỏ một cột ⇒ ĐỎ (E7_COLUMN_MISMATCH)
ok 5 - luat 8: verifier bị làm yếu ⇒ self-test ĐỎ (E8_CASE_GREEN)
# selfbreak: 5/5 ca phá ĐỎ đúng mã; cây thật không đổi
```

`--self-test` (luật 8) chạy mười ba ca: thêm vào log kiểm bằng
`./v2/bin/v2 check-rules --self-test`.

## 6. Giới hạn đã biết (không giấu)

- Bộ tách số giữ **ngữ nghĩa token** của kt76: token số **đứng riêng** mới bị bắt; giá trị dính
  sau `key=` (ví dụ `writes=0`) được coi là định danh trường evidence. Lý do: các tài liệu K6 đã
  qua review (`K6-DOD`, `k6-exec`, `k6-persist`) viết số trong `key=value` ở prose; nếu cấm sẽ phải
  đưa toàn bộ chúng vào ngoại lệ, tức **bỏ kiểm** các báo cáo quan trọng nhất. Các ca phá đã đăng
  ký ở luật 8 không dựa vào giới hạn này; muốn siết thì thêm ca phá + sửa `numeric_tokens` trong
  **cùng một lần** (đúng luật 8).
- Luật 1 không quét `v2/agent/STATE.md` (D2-112d: số của STATE thuộc từng task, đã có verifier
  riêng); luật 3 chỉ thấy bộ đếm được **khai** `counter=<tên>`.
- `v2/docs/k6-parity.md` của **KT-77a** (task song song, chưa `git add`) hiện **chưa đạt** luật 1
  (khối `scope=` chép kèm tiền tố `số-dòng:` nên không nguyên văn — ví dụ `k6-parity.md:85-89`,
  `:96-99`). KT-80 **không** sửa tệp của task khác; đã báo lead để KT-77a sửa trước khi commit.
  Khi tệp vào index, cổng sẽ kiểm nó (không có ngoại lệ cho tài liệu mới).

## 7. Đo và freeze

Wall của năm luật trong cổng: xem `law_wall_ms` ở mục 4 (tổng dưới ngân sách sáu mươi giây của
OD-K6-5). Ảnh sản phẩm **không đổi** (`fd202276…`/188552); `v2 check` PASS; `gate --fast` PASS;
clippy `EXIT=0`; `0` QEMU trước/sau. Freeze: không job nền; lead commit theo pathspec `[KT-80]`.

Một người ghi (D2-105): KT-80 chỉ ghi vùng của mình; `STATE.md`/`BLOCKED.md` cập nhật **append**
để không đè mục KT-77a đang được task song song ghi.
