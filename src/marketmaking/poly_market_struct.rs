use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClobReward {
    pub id: Option<String>,
    pub conditionId: Option<String>,
    pub assetAddress: Option<String>,
    pub rewardsAmount: Option<f64>,
    pub rewardsDailyRate: Option<f64>,
    pub startDate: Option<String>,
    pub endDate: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Tag {
    pub id: Option<String>,
    pub label: Option<String>,
    pub slug: Option<String>,
    // pub forceShow: Option<bool>,
    // pub updatedAt: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Market {
    pub id: Option<String>,
    pub question: Option<String>,
    pub conditionId: Option<String>,
    pub slug: Option<String>,
    pub resolutionSource: Option<String>,
    pub endDate: Option<String>,
    pub liquidity: Option<String>,
    pub startDate: Option<String>,
    pub fee: Option<String>,
    pub image: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub outcomes: Option<String>,
    pub outcomePrices: Option<String>,
    pub volume: Option<String>,
    pub active: Option<bool>,
    pub marketType: Option<String>,
    pub closed: Option<bool>,
    pub marketMakerAddress: Option<String>,
    pub updatedBy: Option<i64>,
    pub createdAt: Option<String>,
    pub updatedAt: Option<String>,
    pub wideFormat: Option<bool>,
    pub new: Option<bool>,
    pub featured: Option<bool>,
    pub submitted_by: Option<String>,
    pub archived: Option<bool>,
    pub resolvedBy: Option<String>,
    pub restricted: Option<bool>,
    pub groupItemTitle: Option<String>,
    pub groupItemThreshold: Option<String>,
    pub questionID: Option<String>,
    pub enableOrderBook: Option<bool>,
    pub orderPriceMinTickSize: Option<f64>,
    pub orderMinSize: Option<i64>,
    pub volumeNum: Option<f64>,
    pub liquidityNum: Option<f64>,
    pub endDateIso: Option<String>,
    pub startDateIso: Option<String>,
    pub hasReviewedDates: Option<bool>,
    pub commentsEnabled: Option<bool>,
    pub volume24hr: Option<f64>,
    pub secondsDelay: Option<i64>,
    pub clobTokenIds: Option<String>,
    pub umaBond: Option<String>,
    pub umaReward: Option<String>,
    pub fpmmLive: Option<bool>,
    pub volume24hrClob: Option<f64>,
    pub volumeClob: Option<f64>,
    pub liquidityClob: Option<f64>,
    pub makerBaseFee: Option<f64>,
    pub takerBaseFee: Option<f64>,
    pub customLiveness: Option<i64>,
    pub acceptingOrders: Option<bool>,
    pub negRisk: Option<bool>,
    pub negRiskMarketID: Option<String>,
    pub negRiskRequestID: Option<String>,
    pub commentCount: Option<i64>,
    pub notificationsEnabled: Option<bool>,
    pub _sync: Option<bool>,
    pub creator: Option<String>,
    pub ready: Option<bool>,
    pub funded: Option<bool>,
    pub cyom: Option<bool>,
    pub competitive: Option<f64>,
    pub pagerDutyNotificationEnabled: Option<bool>,
    pub approved: Option<bool>,
    pub clobRewards: Option<Vec<ClobReward>>,
    pub rewardsMinSize: Option<i64>,
    pub rewardsMaxSpread: Option<f64>,
    pub spread: Option<f64>,
    pub oneDayPriceChange: Option<f64>,
    pub lastTradePrice: Option<f64>,
    pub bestBid: Option<f64>,
    pub bestAsk: Option<f64>,
    pub automaticallyActive: Option<bool>,
    pub clearBookOnStart: Option<bool>,
    #[serde(default, skip_deserializing)]
    pub is_yes_market: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EventJson {
    pub id: Option<String>,
    pub ticker: Option<String>,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resolutionSource: Option<String>,
    pub startDate: Option<String>,
    pub creationDate: Option<String>,
    pub endDate: Option<String>,
    pub image: Option<String>,
    pub icon: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub new: Option<bool>,
    pub featured: Option<bool>,
    pub restricted: Option<bool>,
    pub liquidity: Option<f64>,
    pub volume: Option<f64>,
    pub openInterest: Option<f64>,
    pub sortBy: Option<String>,
    pub reviewStatus: Option<String>,
    pub published_at: Option<String>,
    pub updatedBy: Option<String>,
    pub createdAt: Option<String>,
    pub updatedAt: Option<String>,
    pub commentsEnabled: Option<bool>,
    pub competitive: Option<f64>,
    pub volume24hr: Option<f64>,
    pub featuredImage: Option<String>,
    pub enableOrderBook: Option<bool>,
    pub liquidityClob: Option<f64>,
    pub negRisk: Option<bool>,
    pub negRiskMarketID: Option<String>,
    pub negRiskFeeBips: Option<f64>,
    pub commentCount: Option<i32>,
    pub markets: Option<Vec<Market>>,
    pub enableNegRisk: Option<bool>,
    pub tags: Option<Vec<Tag>>, // Add other fields as needed
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Event {
    pub id: Option<String>,
    pub ticker: Option<String>,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resolutionSource: Option<String>,
    pub startDate: Option<String>,
    pub creationDate: Option<String>,
    pub endDate: Option<String>,
    pub image: Option<String>,
    pub icon: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub new: Option<bool>,
    pub featured: Option<bool>,
    pub restricted: Option<bool>,
    pub liquidity: Option<f64>,
    pub volume: Option<f64>,
    pub openInterest: Option<f64>,
    pub sortBy: Option<String>,
    pub reviewStatus: Option<String>,
    pub published_at: Option<String>,
    pub updatedBy: Option<String>,
    pub createdAt: Option<String>,
    pub updatedAt: Option<String>,
    pub commentsEnabled: Option<bool>,
    pub competitive: Option<f64>,
    pub volume24hr: Option<f64>,
    pub featuredImage: Option<String>,
    pub enableOrderBook: Option<bool>,
    pub liquidityClob: Option<f64>,
    pub negRisk: Option<bool>,
    pub negRiskMarketID: Option<String>,
    pub negRiskFeeBips: Option<f64>,
    pub commentCount: Option<i32>,
    pub market_asset_ids: Option<Vec<String>>,
    pub enableNegRisk: Option<bool>,
    // Add other fields as needed
}

fn expand_market_variants(market: &Market) -> Vec<(String, Market)> {
    if let Some(clob_token_ids) = &market.clobTokenIds {
        if let Ok(asset_ids) = serde_json::from_str::<Vec<String>>(clob_token_ids) {
            return asset_ids
                .into_iter()
                .enumerate()
                .map(|(idx, asset_id)| {
                    let mut market_with_side = market.clone();
                    market_with_side.is_yes_market = match idx {
                        0 => Some(true),
                        1 => Some(false),
                        _ => None,
                    };
                    (asset_id, market_with_side)
                })
                .collect();
        }
    }
    Vec::new()
}

pub fn build_asset_id_to_market_map(events: &[EventJson]) -> HashMap<String, Arc<Market>> {
    let mut asset_id_to_market: HashMap<String, Arc<Market>> = HashMap::new();

    for event in events {
        if let Some(markets) = &event.markets {
            for market in markets {
                for (asset_id, market_with_side) in expand_market_variants(market) {
                    asset_id_to_market.insert(asset_id, Arc::new(market_with_side));
                }
            }
        }
    }

    asset_id_to_market
}

pub fn build_asset_id_to_event_map(events: &Vec<EventJson>) -> HashMap<String, Arc<Event>> {
    let mut asset_id_to_event: HashMap<String, Arc<Event>> = HashMap::new();

    for event_json in events {
        if let Some(negRiskMarketID) = &event_json.negRiskMarketID {
            let event = event_json_to_event(event_json);
            asset_id_to_event.insert(negRiskMarketID.clone(), Arc::new(event));
        }
    }
    asset_id_to_event
}

pub fn event_json_to_event(event_json: &EventJson) -> Event {
    let mut market_ids: Vec<String> = Vec::new();

    if let Some(markets) = &event_json.markets {
        for market in markets {
            market_ids.extend(
                expand_market_variants(market)
                    .into_iter()
                    .map(|(asset_id, _)| asset_id),
            );
        }
    }

    // Construct the Event instance
    let event = Event {
        id: event_json.id.clone(),
        ticker: event_json.ticker.clone(),
        slug: event_json.slug.clone(),
        title: event_json.title.clone(),
        description: event_json.description.clone(),
        resolutionSource: event_json.resolutionSource.clone(),
        startDate: event_json.startDate.clone(),
        creationDate: event_json.creationDate.clone(),
        endDate: event_json.endDate.clone(),
        image: event_json.image.clone(),
        icon: event_json.icon.clone(),
        active: event_json.active,
        closed: event_json.closed,
        archived: event_json.archived,
        new: event_json.new,
        featured: event_json.featured,
        restricted: event_json.restricted,
        liquidity: event_json.liquidity,
        volume: event_json.volume,
        openInterest: event_json.openInterest,
        sortBy: event_json.sortBy.clone(),
        reviewStatus: event_json.reviewStatus.clone(),
        published_at: event_json.published_at.clone(),
        updatedBy: event_json.updatedBy.clone(),
        createdAt: event_json.createdAt.clone(),
        updatedAt: event_json.updatedAt.clone(),
        commentsEnabled: event_json.commentsEnabled,
        competitive: event_json.competitive,
        volume24hr: event_json.volume24hr,
        featuredImage: event_json.featuredImage.clone(),
        enableOrderBook: event_json.enableOrderBook,
        liquidityClob: event_json.liquidityClob,
        negRisk: event_json.negRisk,
        negRiskMarketID: event_json.negRiskMarketID.clone(),
        negRiskFeeBips: event_json.negRiskFeeBips,
        commentCount: event_json.commentCount,
        market_asset_ids: if market_ids.is_empty() {
            None
        } else {
            Some(market_ids)
        },
        enableNegRisk: event_json.enableNegRisk, // Copy other fields as needed
    };
    event
}

pub fn events_json_to_events_with_market_map(
    events: Vec<EventJson>,
) -> (Vec<Event>, HashMap<String, Arc<Market>>) {
    let market_map = build_asset_id_to_market_map(&events);
    let event_vec = events.iter().map(event_json_to_event).collect();
    (event_vec, market_map)
}

pub fn events_json_to_events(events: Vec<EventJson>) -> Vec<Event> {
    events_json_to_events_with_market_map(events).0
}

// pub fn map_asset_ids_to_events(events: &[Event]) -> HashMap<String, Vec<Event>> {
//     let mut asset_id_to_events: HashMap<String, Vec<Event>> = HashMap::new();

//     events
//         .iter()
//         .flat_map(|event| {
//             event
//                 .markets
//                 .as_ref()
//                 .into_iter()
//                 .flat_map(|markets| markets.iter())
//                 .filter_map(|market| {
//                     market
//                         .clobTokenIds
//                         .as_ref()
//                         .map(|clob_token_ids| (market, clob_token_ids))
//                 })
//                 .flat_map(move |(_market, clob_token_ids)| {
//                     // Try parsing as JSON array of strings
//                     let ids = if let Ok(ids) = serde_json::from_str::<Vec<String>>(clob_token_ids) {
//                         ids
//                     } else {
//                         // If parsing fails, assume comma-separated string
//                         clob_token_ids
//                             .split(',')
//                             .map(|id| id.trim().to_string())
//                             .collect::<Vec<_>>()
//                     };

//                     // Return an iterator over (asset_id, event.clone()) pairs
//                     ids.into_iter().map(move |asset_id| (asset_id, event.clone()))
//                 })
//         })
//         .for_each(|(asset_id, event)| {
//             asset_id_to_events
//                 .entry(asset_id)
//                 .or_insert_with(Vec::new)
//                 .push(event);
//         });

//     asset_id_to_events
// }
