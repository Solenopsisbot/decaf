package demo;

import java.net.URI;
import java.nio.ByteBuffer;
import java.util.concurrent.Flow;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;

public final class Probe {
    static String bool(boolean value) { return value ? "T" : "F"; }
    static HttpRequest.BodyPublisher body() {
        return new HttpRequest.BodyPublisher() {
            public long contentLength() { return 0; }
            public void subscribe(Flow.Subscriber<? super ByteBuffer> subscriber) {}
        };
    }

    public static void main(String[] args) { System.out.println(args.length > 0 ? uris() : run()); }

    public static String run() {
        HttpClient.Builder builder = HttpClient.newBuilder();
        HttpClient defaults = builder.build();
        HttpClient.Builder chained = builder.connectTimeout(Duration.ofSeconds(5))
                .version(HttpClient.Version.HTTP_1_1)
                .followRedirects(HttpClient.Redirect.ALWAYS);
        HttpClient client = chained.build();
        HttpRequest.Builder requests = HttpRequest.newBuilder(URI.create("https://example.com/a"));
        HttpRequest request = requests.uri(URI.create("https://example.com/b"))
                .header("X-Test", "yes").GET().build();
        StringBuilder out = new StringBuilder();
        out.append(bool(builder == chained));
        out.append("|").append(defaults.connectTimeout().isEmpty());
        out.append("|").append(defaults.version());
        out.append("|").append(defaults.followRedirects());
        out.append("|").append(client.connectTimeout().orElseThrow().getSeconds());
        out.append("|").append(client.version());
        out.append("|").append(client.followRedirects());
        out.append("|").append(request.uri()).append("|").append(request.method());
        try { HttpRequest.newBuilder().build(); out.append("|no-error"); }
        catch (IllegalStateException e) { out.append("|missing-uri"); }
        try { HttpRequest.newBuilder().header("bad name", "x"); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|bad-header"); }
        try { HttpRequest.newBuilder().header("X", "bad\nvalue"); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|bad-value"); }
        try { builder.connectTimeout(Duration.ZERO); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|zero-").append(e.getMessage()); }
        try { requests.timeout(Duration.ZERO); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|request-zero-").append(e.getMessage()); }
        try { requests.header("Host", "v"); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|restricted-").append(e.getMessage()); }
        try { requests.method("Host", body()); out.append("|method-host"); }
        catch (IllegalArgumentException e) { out.append("|method-host-error"); }
        try { requests.method("", body()); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|method-empty-").append(e.getMessage()); }
        try { requests.method("bad name", body()); out.append("|no-error"); }
        catch (IllegalArgumentException e) { out.append("|method-bad-").append(e.getMessage()); }
        try { requests.version(null); out.append("|no-error"); }
        catch (NullPointerException e) { out.append("|version-null"); }
        try { client.send(request, HttpResponse.BodyHandlers.ofString()); out.append("|no-error"); }
        catch (Exception e) { out.append("|send-").append(e.getClass().getName()); }
        return out.toString();
    }

    /// `HttpRequest.newBuilder(URI)`/`uri(URI)`/`build()` validation (#283 review):
    /// exact messages for null, a missing scheme, a non-HTTP scheme, no host, no URI.
    public static String uris() {
        StringBuilder out = new StringBuilder();
        String[] texts = {"http:foo", "HTTPS://Example.com/x", "http:///path", "mailto:a@b",
            "FTP://x", "relative", "https://user@h:8080/p?q#f"};
        for (String text : texts) {
            try {
                java.net.http.HttpRequest.newBuilder(URI.create(text));
                out.append(text).append(" ok|");
            } catch (RuntimeException e) {
                out.append(text).append(' ').append(e.getClass().getName()).append(':').append(e.getMessage()).append('|');
            }
        }
        try { java.net.http.HttpRequest.newBuilder((URI) null); }
        catch (RuntimeException e) { out.append("new(null) ").append(e.getMessage()).append('|'); }
        try { java.net.http.HttpRequest.newBuilder().uri(null); }
        catch (RuntimeException e) { out.append("uri(null) ").append(e.getMessage()).append('|'); }
        try { java.net.http.HttpRequest.newBuilder().build(); }
        catch (RuntimeException e) { out.append("build ").append(e.getClass().getName()).append(':').append(e.getMessage()); }
        return out.toString();
    }
}
