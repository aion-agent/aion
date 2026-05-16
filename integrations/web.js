const input = await new Response(Deno.stdin.readable).text();
const { query } = JSON.parse(input);

const TAVILY_API_KEY = Deno.env.get("TAVILY_API_KEY");

if (!TAVILY_API_KEY) {
    console.log(JSON.stringify({ error: "TAVILY_API_KEY environment variable is not set." }));
    Deno.exit(0);
}

try {
    const res = await fetch("https://api.tavily.com/search", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ api_key: TAVILY_API_KEY, query, search_depth: "basic" })
    });
    const data = await res.json();
    console.log(JSON.stringify({ status: "success", results: data.results }));
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
}
