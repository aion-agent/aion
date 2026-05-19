const input = await new Response(Deno.stdin.readable).text();
const { action, path, content } = JSON.parse(input);

try {
    if (action === "read") {
        const text = await Deno.readTextFile(path);
        console.log(JSON.stringify({ status: "success", content: text }));
    } else if (action === "write") {
        await Deno.writeTextFile(path, content);
        console.log(JSON.stringify({ status: "success", message: `Wrote to ${path}` }));
    } else {
        console.log(JSON.stringify({ error: "Invalid action. Use 'read' or 'write'." }));
    }
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
}