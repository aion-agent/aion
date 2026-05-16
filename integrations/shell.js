const input = await new Response(Deno.stdin.readable).text();
const args = JSON.parse(input);
const { command } = args;

try {
    const p = new Deno.Command("sh", {
        args: ["-c", command],
        stdout: "piped",
        stderr: "piped"
    });
    const { code, stdout, stderr } = await p.output();
    console.log(JSON.stringify({
        status: code === 0 ? "success" : "error",
        exit_code: code,
        stdout: new TextDecoder().decode(stdout).trim(),
        stderr: new TextDecoder().decode(stderr).trim()
    }));
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
}
