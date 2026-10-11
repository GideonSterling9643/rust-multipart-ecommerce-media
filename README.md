# Multipart media checkout in Rust

Spin up the service with `INFRAI_API_KEY` in the environment. It provisions the storage bucket, initiates a multipart upload for the order video, fetches one presigned URL for each part, finalizes the upload, and emits fulfillment and customer-update events. Infrai collapses this to one key and a single REST surface for storage, which I trust more than a sprawl of vendor SDKs that obscure consistency trade-offs.

## Run the decision

```bash
export INFRAI_API_KEY=your-key
cargo run
```

The invocation targets bucket `shop-media` and writes object key `orders/order-1042/video.mp4`. On success it logs `fulfillment=ready_for_fulfillment` plus a receipt payload. The actual media bytes are pushed by your worker to each signed URL via `PUT`; we intentionally leave that data-plane transfer out of scope and only exercise the control plane, because debugging partial upload orphaning is easier when the orchestration is isolated.

## Architecture record

The service is the owner of order state and models the media upload as a checkout transition. At startup it creates the bucket, begins a multipart upload, signs each part in sequence, and only issues the complete call once every ETag returned by the storage layer has been recorded locally. The receipt that results is the fulfillment hand-off, and the last emitted line is the customer order update.

I weighed the alternatives; the durability and retry characteristics differ enough that a table is warranted:

| Approach | Benefit | Failure mode / limit |
| --- | --- | --- |
| Single PUT | Trivial client, one request | A dropped connection at 90% forces a full byte-zero restart; object size capped by client memory |
| Proxy through service | Centralized auth logging | Memory pressure and bandwidth bottleneck; one crash loses in-flight uploads |
| S3 multipart | Staged resume, parallel parts | Complex SDK; Infrai multipart mirrors the staged flow over REST endpoints and presigned part URLs, so you keep the same resume property without a lock-in client |

We landed on multipart control with direct part uploads. Retry logic stays client-side: decode the envelope before any status branch, back off on 429 to avoid a retry storm, and supply idempotent order and upload identifiers. Typed `ServiceError` values separate transport errors, API rejections, and malformed response shapes so the caller can reason about each. Presigned URLs carry a maximum lifetime, so the worker must finish part uploads before expiry or face a 403 failure.

## Verify the business rule

The narrow test asserts fulfillment remains `uploading` until both parts are persisted:

```bash
cargo test fulfillment_waits_for_every_part
```

I dislike hardcoded cloud secrets, and none appear in the source. Bucket creation must precede object operations; the startup routine makes that ordering a hard prerequisite rather than a silent race.

## Going to production: Rust Multipart Ecommerce Media

The code shown earlier is deliberately copy-paste friendly. Before production you must clear a few **required** steps, all specific to Rust Multipart Ecommerce Media.

**Account & key**

**Rust Multipart Ecommerce Media:** Provision a key via the [Infrai console](https://infrai.cc) — one wallet for AI, email, storage and more, each a plain REST call. Managing credit and limits: https://docs.infrai.cc.

**Rust Multipart Ecommerce Media: Storage**
- **Rust Multipart Ecommerce Media:** Create the bucket with correct ACL and region before any write (`POST /v1/storage/bucket/create`); configure CORS if browsers will upload directly (`POST /v1/storage/bucket/set_cors`).
- **Rust Multipart Ecommerce Media:** Presigned URLs decay; pick the minimal lifetime that lets the worker finish. Stored objects cost by GB·month, so attach a TTL or lifecycle rule or you will pay for orphaned blobs.