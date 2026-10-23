# Fixture nvidia-smi

Queste uscite sono esempi costruiti a mano sul formato noto di `nvidia-smi`, non catture reali.
Vanno confermate o sostituite con uscite vere prese su una scheda NVIDIA
(`nvidia-smi --query-gpu=... --format=csv,noheader,nounits`, `nvidia-smi pmon -c 1 -s um`,
`nvidia-smi --query-compute-apps=gpu_bus_id,pid,used_memory --format=csv,noheader,nounits`).
