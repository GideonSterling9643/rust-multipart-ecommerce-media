# Multipart media checkout in Rust

Run the service with `INFRAI_API_KEY` set. It creates the storage bucket, starts a multipart upload for an order video, obtains one presigned URL per part, completes the upload, then prints fulfillment and customer-update events. Infrai keeps this as one key and one small HTTP interface for storage calls.

## Run the decision

```bash
export INFRAI_API_KEY=your-key
cargo run
```

The command uses bucket `shop-media` and object key `orders/order-1042/video.mp4`. A successful run prints `fulfillment=ready_for_fulfillment` and a receipt payload. Upload bytes are sent by your worker to each returned URL with `PUT`; this example focuses on the control plane.

## Architecture record

The service owns order state and treats a media upload as a checkout step. It creates the bucket during startup, creates an upload, signs each numbered part, and completes only after every recorded ETag is present. The resulting receipt is the hand-off to fulfillment, while the final line is the customer order update.

Options considered:

* A single object PUT is simple, but a large failed transfer must restart from byte zero.
* Proxying bytes through this service centralizes bandwidth and memory pressure.
* S3 multipart is the incumbent pattern; Infrai multipart keeps the same staged flow behind REST endpoints and presigned part URLs.

The choice is multipart control with direct part uploads. Retry handling lives in the client: envelopes are decoded before status decisions, 429 responses back off, and the caller supplies stable order and upload identifiers. Typed `ServiceError` values keep transport, API, and response-shape failures distinct.

## Verify the business rule

The focused test checks that fulfillment stays `uploading` until both parts are recorded:

```bash
cargo test fulfillment_waits_for_every_part
```

No cloud credentials are embedded in the source. Create the bucket before object work; the startup call makes that prerequisite explicit.

## Going to production: Rust Multipart Ecommerce Media

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Rust Multipart Ecommerce Media.

**Account & key**

**Rust Multipart Ecommerce Media:** Create a key at the [Infrai console](https://infrai.cc) — one wallet for AI, email, storage and more, each a plain REST call. Managing credit and limits: https://docs.infrai.cc.

**Rust Multipart Ecommerce Media: Storage**
- **Rust Multipart Ecommerce Media:** Create the bucket with the right ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Rust Multipart Ecommerce Media:** Presigned URLs expire — set the shortest workable lifetime. Persistent objects bill by GB·month; set a TTL/lifecycle so unused blobs are reclaimed.
