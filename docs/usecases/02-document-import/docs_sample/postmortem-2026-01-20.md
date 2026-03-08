---
author: Bob
date: 2026-01-20
---
# Postmortem: Redis Cache Stampede

## Summary

On January 20th, a Redis cache stampede caused a 15-minute outage.
Multiple services simultaneously attempted to rebuild expired cache keys,
overwhelming PostgreSQL with duplicate queries.

## Timeline

- 10:15 UTC: Redis key expiration triggered mass cache miss
- 10:16 UTC: PostgreSQL connection pool exhausted (max 100 connections)
- 10:18 UTC: Prometheus alerts fired for high latency
- 10:22 UTC: On-call engineer identified the stampede pattern
- 10:25 UTC: Applied cache lock mechanism via Redis SETNX
- 10:30 UTC: Service recovered, latency normalized

## Root Cause

No cache stampede protection was implemented. When a popular cache key
expired, all concurrent requests hit PostgreSQL simultaneously.

## Action Items

- Implement probabilistic early expiration in Redis client
- Add circuit breaker between services and PostgreSQL
- Set up Grafana dashboard for cache hit ratio monitoring
- Review Sentinel configuration for faster failover
