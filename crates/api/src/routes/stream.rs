use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::error::ApiResult;
use crate::routes::runs::parse_run_id;
use crate::state::ApiState;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StreamQuery {
    pub after: Option<u64>,
}

pub async fn stream_events(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let run_id = parse_run_id(&run_id)?;
    let after = query.after.unwrap_or(0);
    let receiver = state.runtime.events.subscribe();
    let backlog = state
        .runtime
        .service
        .events(run_id, after, 2_000)
        .await?
        .events;

    let historical = tokio_stream::iter(backlog.into_iter().map(move |event| {
        Ok(Event::default()
            .id(event.sequence.to_string())
            .event(event.event_type.clone())
            .json_data(event)
            .unwrap_or_else(|_| Event::default().comment("event could not be encoded")))
    }));

    let live = BroadcastStream::new(receiver).filter_map(move |result| match result {
        Ok(event) if event.run_id == run_id && event.sequence > after => Some(Ok(Event::default()
            .id(event.sequence.to_string())
            .event(event.event_type.clone())
            .json_data(event)
            .unwrap_or_else(|_| Event::default().comment("event could not be encoded")))),
        Ok(_) => None,
        Err(_) => Some(Ok(Event::default()
            .event("stream_lagged")
            .data("the live stream lagged, reconnect with the last sequence"))),
    });

    Ok(Sse::new(historical.chain(live)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
