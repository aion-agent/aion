const input = await new Response(Deno.stdin.readable).text();
const args = JSON.parse(input);

const { location } = args;

// Simulate a weather API
const weather = {
    "New York": "Sunny, 22°C",
    "London": "Rainy, 14°C",
    "Tokyo": "Cloudy, 18°C",
    "Istanbul": "Clear, 25°C"
};

const result = weather[location] || `Unknown weather for ${location}. Try New York, London, Tokyo, or Istanbul.`;

// Send JSON back to Aion via stdout
console.log(JSON.stringify({ location, weather: result }));
