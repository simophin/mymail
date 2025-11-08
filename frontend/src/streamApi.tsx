import {Observable, Subject, Subscription} from "rxjs";


export function streamWebSocketApi<T, S = any>(url: string, messageToSend?: Subject<S | undefined>): Observable<T> {
    return new Observable<T>((subscriber) => {
        const ws = new WebSocket(url);
        console.log(Date.now(), url, "connecting");

        ws.onmessage = (event) => {
            console.log(Date.now(), url, "Received message", event.data);
            subscriber.next(JSON.parse(event.data));
        };

        ws.onerror = (event) => {
            subscriber.error(event);
        };

        const subscriptions: Subscription[] = [];

        ws.onopen = () => {
            if (messageToSend) {
                subscriptions.push(messageToSend.subscribe((message) => {
                    if (typeof message !== "undefined") {
                        ws.send(JSON.stringify(message));
                    }
                }));
            }
        };

        ws.onclose = () => {
            subscriber.error("WebSocket closed");
        };

        return () => {
            for (const subscription of subscriptions) {
                subscription.unsubscribe();
            }
            ws.close();
        };
    });
}