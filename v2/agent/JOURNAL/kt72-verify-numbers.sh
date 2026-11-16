#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-72 (D2-110A) — kiểm chéo số đo, **theo TỪNG tài liệu**, **quét mọi số** (không allowlist
# token rời). Mỗi số trong một tài liệu phải xuất hiện trong:
#   (a) log ĐÃ CHÉP VÀO REPO của task (`v2/agent/JOURNAL/kt72-*.log`, trừ log verify này), hoặc
#   (b) `v2/test/boot/expected.txt`, hoặc
#   (c) **mã nguồn** (hằng thiết kế `[TK]`/dữ kiện nền tảng) — chỉ các tệp thiết bị/FS liên quan.
# Ngoài ra mỗi tài liệu còn danh sách **required** (chống xoá số): thiếu là ĐỎ.
# Chạy: bash v2/agent/JOURNAL/kt72-verify-numbers.sh        (exit 0 = sạch)
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/../../.." || exit 2
python3 - <<'PY'
import glob
import re
import sys

J = 'v2/agent/JOURNAL'

def scope_state(text):
    """STATE.md: khối trạng thái đầu (HEAD..Nợ) + mục `## KT-72 —` tới hết."""
    start = text.find('```\nHEAD')
    end = text.find('```\n```', start)
    top = text[start:end] if start >= 0 and end > start else ''
    kt72 = ''
    marker = '## KT-72 —'
    pos = text.find(marker)
    if pos >= 0:
        kt72 = text[pos:]
    return top + '\n' + kt72

def scope_k3b(text):
    pos = text.find('## 20. KT-72')
    return text[pos:] if pos >= 0 else ''

def scope_whole(text):
    return text

DOCS = [
    ('v2/docs/k6-virtio-blk.md', scope_whole,
     ['data+bss=242840', 'size=16384', 'content_match=1', 'trace-compare: 61/61',
      'KTEST: 15/15', 'soak: 0/1000', 'id_timeout_proven=0']),
    ('v2/docs/k3b-core.md', scope_k3b,
     ['capacity=8192', 'found=0', 'flush_feature=1', '0/1000', '61/61']),
    ('v2/agent/STATE.md', scope_state,
     ['242840', '10976', '200184', '718', '0/1000']),
    (f'{J}/kt72-journal.md', scope_whole,
     ['writes=16 reads=16', 'refused_no_request=1', 'lba1_magic=VF04', '0/1000', '61/61', '718']),
]

SOURCES = (
    glob.glob('v2/lib/vifast-virtio/src/*.rs')
    + glob.glob('v2/lib/vifast-virtio/tests/*.rs')
    + ['v2/kernel/vifast-plat-riscv64/src/pci.rs',
       'v2/kernel/vifast-plat-riscv64/src/virtio_blk.rs',
       'v2/kernel/vifast-plat-riscv64/src/mmu.rs',
       'v2/kernel/vifast-plat-riscv64/src/lib.rs',
       'v2/kernel/vifast-kbin-riscv64/src/ktest_vblk.rs',
       'v2/kernel/vifast-kbin-riscv64/src/ktest_obj.rs',
       'v2/lib/vifast-fs/src/fs.rs',
       'v2/test/boot/ktest.sh',
       'v2/test/boot/expected.txt']
)
LOG_FILES = [p for p in sorted(glob.glob(f'{J}/kt72-*.log')) if 'verify-numbers' not in p]

def read_all(paths):
    out = []
    for p in paths:
        try:
            with open(p, 'r', encoding='utf-8', errors='replace') as fh:
                out.append(fh.read())
        except OSError:
            pass
    return '\n'.join(out)

LOG_CORPUS = read_all(LOG_FILES) + '\n' + (open('v2/test/boot/expected.txt').read()
                                           if glob.glob('v2/test/boot/expected.txt') else '')
SRC_CORPUS = read_all(SOURCES)

def squash(s):
    return s.replace('_', '').lower()

LOG_C = squash(LOG_CORPUS)
SRC_C = squash(SRC_CORPUS)

# Bỏ các **định danh** (không phải số đo) trước khi quét: D2-*, KT-*, §x.y, file:line, path, URL,
# version x.y.z, số trong tên tệp (kt72-*, k6-*, vifast-*).
CLEAN = [
    (re.compile(r'\bD2-?\d+[a-zA-Z]?\b'), ' '),
    (re.compile(r'\b(?:KT|ST|VD|VF)-\d+[a-zA-Z]?\b'), ' '),
    (re.compile(r'§\s*[0-9.]+'), ' '),
    (re.compile(r'\b[A-Za-z_][A-Za-z0-9_.-]*\.(?:rs|md|sh|toml|ld|txt|log|img)(?::\d+)?'), ' '),
    (re.compile(r'\./[A-Za-z0-9_./-]+'), ' '),
    (re.compile(r'\b[A-Za-z0-9_-]*\d+[A-Za-z0-9_-]*\.(?:rs|md|sh|toml|ld)\b'), ' '),
    (re.compile(r'\b\d+\.\d+\.\d+\b'), ' '),
    (re.compile(r'\bQ\d+\b'), ' '),
]

TOKEN_RE = re.compile(r'(?<![0-9A-Za-z_])(0x[0-9a-fA-F][0-9a-fA-F_]*|\d[\d_]*)(?![0-9A-Za-z_])')

def tokens_of(text):
    for pat, repl in CLEAN:
        text = pat.sub(repl, text)
    out = []
    for line in text.splitlines():
        if line.lstrip().startswith('#'):
            continue  # tiêu đề mục, không phải số đo
        for m in TOKEN_RE.finditer(line):
            tok = m.group(1).replace('_', '').lower()
            out.append(tok)
    return out

def in_corpus(tok, corpus):
    if tok in corpus:
        return True
    if tok.startswith('0x') and tok[2:] in corpus:
        return True
    return False

fails = 0
checked = 0
for path, scope, required in DOCS:
    text = scope(open(path, encoding='utf-8').read())
    missing_req = [r for r in required if r not in text]
    toks = tokens_of(text)
    uniq = sorted(set(toks))
    bad = [t for t in uniq if not in_corpus(t, LOG_C) and not in_corpus(t, SRC_C)]
    via_log = sum(1 for t in uniq if in_corpus(t, LOG_C))
    via_src = sum(1 for t in uniq if not in_corpus(t, LOG_C) and in_corpus(t, SRC_C))
    print(f'== {path}: tokens={len(uniq)} via_log={via_log} via_source={via_src} '
          f'bad={len(bad)}')
    if missing_req:
        print(f'   THIẾU required: {missing_req}')
        fails += len(missing_req)
    for t in bad:
        print(f'   KHÔNG có trong log/mã nguồn: "{t}"')
    fails += len(bad)
    checked += len(uniq)
print('---')
print(f'# verify D2-110A: docs={len(DOCS)} tokens={checked} logs={len(LOG_FILES)} '
      f'sources={len(SOURCES)} fail={fails}')
sys.exit(0 if fails == 0 else 1)
PY
