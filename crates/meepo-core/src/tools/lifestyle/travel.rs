//! Travel & Commute Assistant tools
//!
//! Weather forecasts, directions, flight status monitoring, and packing lists.
//! Cross-references calendar and email for travel context.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tavily::TavilyClient;
use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Get weather forecast
pub struct GetWeatherTool {
    tavily: Option<Arc<TavilyClient>>,
}

impl GetWeatherTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>) -> Self {
        Self { tavily }
    }
}

#[async_trait]
impl ToolHandler for GetWeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn description(&self) -> &str {
        "Get the current weather and forecast for a location. Uses web search to find the \
         latest weather data. Useful for planning outdoor activities or travel."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "location": {
                    "type": "string",
                    "description": "City name or location (e.g., 'San Francisco', 'Tokyo, Japan')"
                },
                "days": {
                    "type": "number",
                    "description": "Number of forecast days (default: 3, max: 7)"
                }
            }),
            vec!["location"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let location = input
            .get("location")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'location' parameter"))?;
        let days = input
            .get("days")
            .and_then(|v| v.as_u64())
            .unwrap_or(3)
            .min(7);

        if location.len() > 200 {
            return Err(anyhow::anyhow!("Location too long (max 200 characters)"));
        }

        debug!("Getting weather for: {} ({} days)", location, days);

        if let Some(ref tavily) = self.tavily {
            let query = format!("weather forecast {} next {} days", location, days);
            let results = tavily.search(&query, 3).await?;
            Ok(format!(
                "Weather search results for {} ({} day forecast):\n\n{}\n\n\
                 Please extract and format the weather information including:\n\
                 - Current conditions (temperature, humidity, wind)\n\
                 - Daily forecast for the next {} days\n\
                 - Any weather alerts or advisories",
                location,
                days,
                TavilyClient::format_results(&results),
                days
            ))
        } else {
            Ok(format!(
                "Web search not available (no Tavily API key). \
                 Cannot fetch weather for {}. Configure TAVILY_API_KEY to enable.",
                location
            ))
        }
    }
}

/// Get directions between locations
pub struct GetDirectionsTool {
    tavily: Option<Arc<TavilyClient>>,
}

impl GetDirectionsTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>) -> Self {
        Self { tavily }
    }
}

#[async_trait]
impl ToolHandler for GetDirectionsTool {
    fn name(&self) -> &str {
        "get_directions"
    }

    fn description(&self) -> &str {
        "Get directions and estimated travel time between two locations. Searches for route \
         information including driving, transit, and walking options."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "from": {
                    "type": "string",
                    "description": "Starting location"
                },
                "to": {
                    "type": "string",
                    "description": "Destination location"
                },
                "mode": {
                    "type": "string",
                    "description": "Travel mode: driving, transit, walking, cycling (default: driving)"
                },
                "depart_time": {
                    "type": "string",
                    "description": "Departure time for traffic estimates (e.g., 'now', '8:00 AM tomorrow')"
                }
            }),
            vec!["from", "to"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let from = input
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'from' parameter"))?;
        let to = input
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' parameter"))?;
        let mode = input
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("driving");
        let depart_time = input
            .get("depart_time")
            .and_then(|v| v.as_str())
            .unwrap_or("now");

        if from.len() > 500 || to.len() > 500 {
            return Err(anyhow::anyhow!("Location too long (max 500 characters)"));
        }

        debug!("Getting directions: {} -> {} ({})", from, to, mode);

        if let Some(ref tavily) = self.tavily {
            let query = format!(
                "directions {} from {} to {} depart {}",
                mode, from, to, depart_time
            );
            let results = tavily.search(&query, 3).await?;
            Ok(format!(
                "Directions search ({} from {} to {}):\n\n{}\n\n\
                 Please extract:\n\
                 - Estimated travel time\n\
                 - Distance\n\
                 - Route summary\n\
                 - Suggested departure time to arrive on time",
                mode,
                from,
                to,
                TavilyClient::format_results(&results)
            ))
        } else {
            Ok(format!(
                "Web search not available. For directions from {} to {} by {}, \
                 try opening Maps: use open_app with 'Maps' or browser_open_tab with a maps URL.",
                from, to, mode
            ))
        }
    }
}

/// Check flight status
pub struct FlightStatusTool {
    tavily: Option<Arc<TavilyClient>>,
    db: Arc<KnowledgeDb>,
}

impl FlightStatusTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>, db: Arc<KnowledgeDb>) -> Self {
        Self { tavily, db }
    }
}

#[async_trait]
impl ToolHandler for FlightStatusTool {
    fn name(&self) -> &str {
        "flight_status"
    }

    fn description(&self) -> &str {
        "Check the status of a flight by flight number. Returns departure/arrival times, \
         delays, gate information, and terminal details. Can also scan emails for upcoming \
         flight confirmations."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "flight_number": {
                    "type": "string",
                    "description": "Flight number (e.g., 'UA123', 'AA456'). Omit to scan emails for flights."
                },
                "date": {
                    "type": "string",
                    "description": "Flight date (default: today). Format: YYYY-MM-DD"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let flight_number = input.get("flight_number").and_then(|v| v.as_str());
        let date = input
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

        if let Some(flight) = flight_number {
            if flight.len() > 20 {
                return Err(anyhow::anyhow!(
                    "Flight number too long (max 20 characters)"
                ));
            }

            debug!("Checking flight status: {} on {}", flight, date);

            if let Some(ref tavily) = self.tavily {
                let query = format!("flight status {} {}", flight, date);
                let results = tavily.search(&query, 5).await?;

                // Store in knowledge graph for tracking
                let _ = self
                    .db
                    .insert_entity(
                        &format!("flight:{}:{}", flight, date),
                        "flight",
                        Some(serde_json::json!({
                            "flight_number": flight,
                            "date": date,
                            "checked_at": chrono::Utc::now().to_rfc3339(),
                        })),
                    )
                    .await;

                Ok(format!(
                    "Flight status search for {} on {}:\n\n{}\n\n\
                     Please extract:\n\
                     - Departure: time, airport, terminal, gate\n\
                     - Arrival: time, airport, terminal, gate\n\
                     - Status: on time / delayed / cancelled\n\
                     - Delay details if applicable",
                    flight,
                    date,
                    TavilyClient::format_results(&results)
                ))
            } else {
                Ok(format!(
                    "Web search not available. Cannot check status for flight {}. \
                     Configure TAVILY_API_KEY to enable.",
                    flight
                ))
            }
        } else {
            // Scan for flights in knowledge graph / emails
            let flights = self
                .db
                .search_entities("flight:", Some("flight"))
                .await
                .unwrap_or_default();

            if flights.is_empty() {
                Ok("No flight number provided and no tracked flights found. \
                     Provide a flight_number or use read_emails to scan for flight confirmations."
                    .to_string())
            } else {
                let flight_list = flights
                    .iter()
                    .take(5)
                    .map(|f| {
                        let meta = f.metadata.as_ref();
                        let num = meta
                            .and_then(|m| m.get("flight_number"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");
                        let dt = meta
                            .and_then(|m| m.get("date"))
                            .and_then(|d| d.as_str())
                            .unwrap_or("unknown");
                        format!("- {} on {}", num, dt)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(format!(
                    "Tracked flights:\n{}\n\n\
                     To check status, call flight_status with a specific flight_number.",
                    flight_list
                ))
            }
        }
    }
}

/// Generate a packing list for a trip
pub struct PackingListTool {
    db: Arc<KnowledgeDb>,
}

impl PackingListTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for PackingListTool {
    fn name(&self) -> &str {
        "packing_list"
    }

    fn description(&self) -> &str {
        "Generate a smart packing list for a trip. Considers destination, duration, weather, \
         trip type (business/leisure), and any special activities. Cross-references calendar \
         events at the destination for context."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "destination": {
                    "type": "string",
                    "description": "Trip destination"
                },
                "duration_days": {
                    "type": "number",
                    "description": "Trip duration in days"
                },
                "trip_type": {
                    "type": "string",
                    "description": "Trip type: business, leisure, mixed (default: leisure)"
                },
                "activities": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Planned activities (e.g., ['hiking', 'swimming', 'formal dinner'])"
                },
                "weather": {
                    "type": "string",
                    "description": "Expected weather (e.g., 'hot and sunny', 'cold and rainy'). Auto-fetched if omitted."
                }
            }),
            vec!["destination", "duration_days"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let destination = input
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'destination' parameter"))?;
        let duration = input
            .get("duration_days")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'duration_days' parameter"))?;
        let trip_type = input
            .get("trip_type")
            .and_then(|v| v.as_str())
            .unwrap_or("leisure");
        let activities: Vec<String> = input
            .get("activities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let weather = input.get("weather").and_then(|v| v.as_str());

        if destination.len() > 200 {
            return Err(anyhow::anyhow!("Destination too long (max 200 characters)"));
        }

        debug!(
            "Generating packing list: {} for {} days ({})",
            destination, duration, trip_type
        );

        // Store trip in knowledge graph
        let _ = self
            .db
            .insert_entity(
                &format!("trip:{}", destination),
                "trip",
                Some(serde_json::json!({
                    "destination": destination,
                    "duration_days": duration,
                    "trip_type": trip_type,
                    "activities": activities,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await;

        Ok(format!(
            "# Packing List Request\n\n\
             - Destination: {}\n\
             - Duration: {} days\n\
             - Trip type: {}\n\
             - Activities: {}\n\
             - Weather: {}\n\n\
             Please generate a comprehensive packing list organized by category:\n\
             1. **Clothing** — based on weather, duration, and activities\n\
             2. **Toiletries** — essentials and travel-size items\n\
             3. **Electronics** — chargers, adapters, devices\n\
             4. **Documents** — passport, tickets, reservations\n\
             5. **Activity-specific** — gear for planned activities\n\
             6. **Miscellaneous** — snacks, medications, comfort items\n\n\
             Consider the trip type ({}) when suggesting clothing formality.\n\
             {}",
            destination,
            duration,
            trip_type,
            if activities.is_empty() {
                "none specified".to_string()
            } else {
                activities.join(", ")
            },
            weather.unwrap_or("not specified — use get_weather to check"),
            trip_type,
            if weather.is_none() {
                "Tip: Use get_weather first to check conditions at the destination."
            } else {
                ""
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_travel.db")).unwrap())
    }

    #[test]
    fn test_get_weather_schema() {
        let tool = GetWeatherTool::new(None);
        assert_eq!(tool.name(), "get_weather");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"location".to_string()));
    }

    #[test]
    fn test_get_directions_schema() {
        let tool = GetDirectionsTool::new(None);
        assert_eq!(tool.name(), "get_directions");
    }

    #[test]
    fn test_flight_status_schema() {
        let tool = FlightStatusTool::new(None, test_db());
        assert_eq!(tool.name(), "flight_status");
    }

    #[test]
    fn test_packing_list_schema() {
        let tool = PackingListTool::new(test_db());
        assert_eq!(tool.name(), "packing_list");
    }
}
