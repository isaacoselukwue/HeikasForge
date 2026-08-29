# Event protocol

## Position

`events.jsonl` inside a run directory is the authoritative history. `state.json`, `manifest.json` and `metrics.json` are rebuildable caches. Any disagreement is resolved in favour of the event log.

## Record shape

Every record carries a schema version, a monotonically increasing sequence starting at one, a sortable event identifier, the run identifier, the candidate, node and attempt where they apply, a UTC timestamp with nanosecond precision, the event type, the previous chain hash, the payload hash and the payload itself.

The payload hash is BLAKE3 over the canonical JSON encoding of the payload. The chain hash is BLAKE3 over the sequence, event identifier, timestamp, previous chain hash and payload hash. The first record links to a genesis hash of sixty-four zeroes.

This detects three distinct corruptions:

- a mutated payload fails the payload hash;
- a reordered or duplicated record fails the sequence check;
- a truncated or spliced history fails the previous hash check.

## Append protocol

An append is a single line followed by a newline, written to a file opened for append, flushed and synchronised before the call returns. The final newline is required. The in-memory tail is only advanced after the synchronised write succeeds, so a crash during the write leaves the tail unchanged.

## Partial records

A process that dies mid-write can leave an incomplete final line. Recovery reads records one line at a time. A line that fails to decode is copied to `events.quarantine.jsonl`, and if it is the final line the log is truncated back to the last complete record. A partial record is never treated as committed, and appending continues from the last verified sequence.

## Atomic node attempt protocol

For every node attempt the dispatcher performs the following sequence. The run lock is held by exactly one dispatcher for the whole dispatch; the event append and projection replacement are additionally serialised by an in-process mutex so that no other candidate task can interleave.

1. Append and synchronise `NodeStarted`.
2. Run the node body outside the critical section, bounded by its class timeout and the cancellation signal.
3. Append and synchronise any evidence events the node produced.
4. Build and validate the `NodeResult`.
5. Assemble the attempt directory in a temporary sibling, synchronise it, then rename it into place. A destination that already exists is an error, so completed evidence is never overwritten.
6. Append and synchronise exactly one terminal event.
7. Apply the event to the in-memory projection.
8. Replace `state.json`, `manifest.json` and `metrics.json` by write, synchronise and rename.
9. Publish the committed event to live subscribers.

## Atomic file replacement

Every projection write creates a temporary file in the destination directory, writes it, synchronises the file, renames it over the destination and then synchronises the parent directory. A reader therefore sees either the previous complete document or the new complete document, never a partial one.

## Reading

Any read path reconciles the stored projection with the durable log before returning it. The projection is loaded, newer events are replayed onto it, and the result is returned. A projection that is missing entirely is rebuilt from sequence zero. This means a crash between the terminal event and the projection replacement is invisible to every reader.
