# T1 catalog-embedding spike — results

Catalog: 1477 items; queries: 44 labeled (hand-authored against the catalog itself; no bench corpus vendored).
CPU-only on every host; host column identifies the machine.

| host | model | template | top1 | top5 | sec1 | q ms (med/p95) | precompute s | RSS peak MiB | margin ans/ask | nomatch-vs-match sim |
|---|---|---|---|---|---|---|---|---|---|---|
| pi5 | BAAI/bge-small-en-v1.5 | desc | 0.889 | 0.944 | 0.944 | 238.0/315.5 | 36.59 | 1008.6 | 0.1113/0.0164 | max 0.6862 vs min 0.7072 |
| pi5 | BAAI/bge-small-en-v1.5 | sec_desc | 0.972 | 0.972 | 1.0 | 161.4/230.9 | 48.78 | 1008.6 | 0.1029/0.0065 | max 0.6549 vs min 0.7073 |
| pi5 | BAAI/bge-small-en-v1.5 | full | 0.972 | 0.972 | 1.0 | 149.3/185.2 | 46.55 | 1008.6 | 0.109/0.0104 | max 0.6586 vs min 0.7202 |
| pi5 | intfloat/e5-small-v2 | desc | 0.861 | 0.944 | 0.917 | 185.6/457.6 | 33.24 | 1024.4 | 0.0382/0.004 | max 0.8187 vs min 0.831 |
| pi5 | intfloat/e5-small-v2 | sec_desc | 0.944 | 0.972 | 0.972 | 210.7/273.7 | 60.77 | 1024.4 | 0.0356/0.0022 | max 0.8183 vs min 0.8268 |
| pi5 | intfloat/e5-small-v2 | full | 0.944 | 0.972 | 0.972 | 359.0/503.2 | 68.56 | 1024.4 | 0.0317/0.0018 | max 0.8211 vs min 0.8285 |
| pi5 | sentence-transformers/all-MiniLM-L6-v2 | desc | 0.778 | 0.889 | 0.917 | 154.4/244.0 | 25.93 | 962.8 | 0.2457/0.0195 | max 0.4799 vs min 0.3624 |
| pi5 | sentence-transformers/all-MiniLM-L6-v2 | sec_desc | 0.889 | 0.944 | 0.972 | 126.4/148.7 | 25.48 | 962.8 | 0.1595/0.0197 | max 0.4155 vs min 0.5495 |
| pi5 | sentence-transformers/all-MiniLM-L6-v2 | full | 0.944 | 0.972 | 1.0 | 127.7/163.0 | 29.03 | 962.8 | 0.1685/0.0175 | max 0.4821 vs min 0.5571 |
| r2-i3n305 | BAAI/bge-small-en-v1.5 | desc | 0.889 | 0.944 | 0.944 | 155.6/190.4 | 7.46 | 1186.9 | 0.1113/0.0164 | max 0.6862 vs min 0.7072 |
| r2-i3n305 | BAAI/bge-small-en-v1.5 | sec_desc | 0.972 | 0.972 | 1.0 | 155.8/192.7 | 8.5 | 1186.9 | 0.1029/0.0065 | max 0.6549 vs min 0.7073 |
| r2-i3n305 | BAAI/bge-small-en-v1.5 | full | 0.972 | 0.972 | 1.0 | 152.2/164.3 | 9.38 | 1186.9 | 0.109/0.0104 | max 0.6586 vs min 0.7202 |
| r2-i3n305 | intfloat/e5-small-v2 | desc | 0.861 | 0.944 | 0.917 | 158.4/186.1 | 8.49 | 1203.9 | 0.0382/0.004 | max 0.8187 vs min 0.831 |
| r2-i3n305 | intfloat/e5-small-v2 | sec_desc | 0.944 | 0.972 | 0.972 | 157.9/226.0 | 12.87 | 1203.9 | 0.0356/0.0022 | max 0.8183 vs min 0.8268 |
| r2-i3n305 | intfloat/e5-small-v2 | full | 0.944 | 0.972 | 0.972 | 160.0/291.4 | 14.87 | 1203.9 | 0.0317/0.0018 | max 0.8211 vs min 0.8285 |
| r2-i3n305 | sentence-transformers/all-MiniLM-L6-v2 | desc | 0.778 | 0.889 | 0.917 | 149.5/165.4 | 2.99 | 1178.1 | 0.2457/0.0195 | max 0.4799 vs min 0.3624 |
| r2-i3n305 | sentence-transformers/all-MiniLM-L6-v2 | sec_desc | 0.889 | 0.944 | 0.972 | 151.2/163.9 | 3.82 | 1178.1 | 0.1595/0.0197 | max 0.4155 vs min 0.5495 |
| r2-i3n305 | sentence-transformers/all-MiniLM-L6-v2 | full | 0.944 | 0.972 | 1.0 | 149.4/156.9 | 4.93 | 1178.1 | 0.1685/0.0175 | max 0.4821 vs min 0.5571 |
| r2-i3n305 | thenlper/gte-small | desc | 0.861 | 0.944 | 0.917 | 17.7/22.0 | 35.36 | 1027.9 | 0.0659/0.0063 | max 0.8701 vs min 0.8467 |
| r2-i3n305 | thenlper/gte-small | sec_desc | 0.944 | 1.0 | 0.972 | 18.0/31.8 | 46.43 | 1027.9 | 0.0603/0.0054 | max 0.8413 vs min 0.8398 |
| r2-i3n305 | thenlper/gte-small | full | 0.944 | 0.972 | 0.972 | 18.0/28.7 | 56.78 | 1027.9 | 0.0603/0.0044 | max 0.8315 vs min 0.8457 |

## Environment pins (both hosts)

numpy==2.5.2, sentence-transformers==5.7.0, tokenizers==0.22.2, torch==2.13.0, transformers==5.14.1 (CPU wheels).
Hosts: pi5 = Raspberry Pi 5 (4x Cortex-A76, aarch64, session-contended); r2-i3n305 = Intel i3-N305 (8 E-cores, AVX2, idle).
Pi gte-small arm was still running at report time; its row is absent from the pi set.
