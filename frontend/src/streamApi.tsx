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

        fetchStream();

        return () => {
            controller.abort();
        };
    });
}