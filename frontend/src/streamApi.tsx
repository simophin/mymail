import { Observable } from "rxjs";


export function streamApi<T>(request: RequestInfo | URL): Observable<T> {
    return new Observable<T>((subscriber) => {
        const controller = new AbortController();

        async function fetchStream() {
            try {
                const res = await fetch(request, {
                    signal: controller.signal,
                });
                const reader = res.body!.getReader();
                const decoder = new TextDecoder();

                while (true) {
                    const { done, value } = await reader.read();
                    if (done) break;

                    subscriber.next(JSON.parse(decoder.decode(value)));
                }
            } catch (error) {
                subscriber.error(error);
            }
        }

        let _ = fetchStream();

        return () => {
            controller.abort();
        };
    });
}

export function streamWebSocketApi<T>(url: string): Observable<T> {
    return new Observable<T>((subscriber) => {
        const ws = new WebSocket(url);

        ws.onmessage = (event) => {
            subscriber.next(JSON.parse(event.data));
        };

        ws.onerror = (event) => {
            subscriber.error(event);
        };

        return () => {
            ws.close();
        };
    });
}