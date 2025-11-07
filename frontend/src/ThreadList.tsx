import {createStore} from "solid-js/store/types/server.js";
import LazyLoadingList, {Props as ListProps} from "./LazyLoadingList";
import {splitProps} from "solid-js";
import {streamApi, streamWebSocketApi} from "./streamApi";
import * as rx from "rxjs";
import {retry, retryWhen} from "rxjs";

const apiUrl = import.meta.env.VITE_BASE_URL;

export type EmailSort = {
    column: string;
    asc: boolean;
}


export type ThreadQuery = {
    accountId: AccountId,
    mailboxId: string;
}

export type AccountId = number;

export type Email = {
    id: string;
    subject: string;
    receivedAt: string
}

export type Thread = {
    id: string;
    emails: Email[];
}


export default function ThreadList(props: {
    query: ThreadQuery,
} & Omit<Omit<ListProps<Thread>, "children">, "watchPage">) {
    const [localProps, listProps] = splitProps(props, ["query"]);
    const watchPage = (offset: number, limit: number) => {
        return streamWebSocketApi<Thread[]>(`${apiUrl}/threads/${localProps.query.accountId}?mailbox_id=${localProps.query.mailboxId}&offset=${offset}&limit=${limit}`)
            .pipe(
                retry({ count: Infinity, delay: 1000 })
            )
    };

    return <LazyLoadingList watchPage={watchPage} {...listProps}>
        {(thread) => (
            <div>{thread?.emails?.at(0)?.subject}</div>
        )}
    </LazyLoadingList>;
}