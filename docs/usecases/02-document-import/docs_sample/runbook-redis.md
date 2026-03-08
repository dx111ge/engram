---
author: Alice
date: 2026-01-05
---
# Redis Runbook

Redis is configured with maxmemory-policy allkeys-lru. When memory pressure
occurs, keys are evicted using LRU. The Redis instance runs on port 6379.
Sentinel monitors Redis for automatic failover. Prometheus scrapes Redis
metrics via redis_exporter.
