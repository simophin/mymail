import {createStore} from "solid-js/store";
import LazyLoadingList, {Page, Props as ListProps} from "./LazyLoadingList";
import {createEffect, createSignal, onCleanup, onMount, splitProps, untrack} from "solid-js";
import {streamWebSocketApi} from "./streamApi";
import * as rx from "rxjs";
import {BehaviorSubject, retry, Subject, Subscription} from "rxjs";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

export type EmailSort = {
    column: 'Date';
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

type EmailQuery = {
    anchor_id?: string,
    mailbox_id?: string,
    search_keyword?: string,
    sorts: EmailSort[],
    limit: number,
};
type EmailSyncState = {};

export default function ThreadList(props: {
    query: ThreadQuery,
} & Pick<ListProps<Thread>, "style"> & Pick<ListProps<Thread>, "class">) {
    const [localProps, listProps] = splitProps(props, ["query"]);
    const watchingPages = createSignal(new Set<number>([0]));
    const watchPage = (offset: number, limit: number) => {
        return streamWebSocketApi<Thread[]>(`${apiUrl}/threads/${localProps.query.accountId}?mailbox_id=${localProps.query.mailboxId}&offset=${offset}&limit=${limit}`)
            .pipe(
                retry({ count: Infinity, delay: 1000 })
            )
    };
    const pageStore = createStore<Page<Thread>[]>([]);
    const numPerPage = 100;

    const emailSyncQuery = new BehaviorSubject<EmailQuery | undefined>(undefined)

    createEffect(() => {
        const sub = streamWebSocketApi<EmailSyncState>(`${apiUrl}/mailboxes/sync/${localProps.query.accountId}/${localProps.query.mailboxId}`, emailSyncQuery)
            .pipe(retry({ count: Infinity, delay: 1000 }))
            .subscribe((syncState) => {
                console.log("Email sync state", syncState);
            });

        onCleanup(() => sub.unsubscribe());
        return sub;
    });

    createEffect(() => {
        let minPage: number | undefined, maxPage: number | undefined;
        (watchingPages[0])().forEach((page) => {
            if (typeof minPage == "undefined" || page < minPage) {
                minPage = page;
            }

            if (typeof maxPage == "undefined" || page > maxPage) {
                maxPage = page;
            }
        });

        if (minPage == undefined || maxPage == undefined) {
            return;
        }

        let anchor_id: string | undefined = undefined;
        let limit: number;
        const pages = untrack(() => pageStore[0]);
        anchor_id = last(pages?.at(minPage)?.at(0)?.emails)?.id;

        console.log("Watching offset", minPage * numPerPage, "to", (maxPage + 1) * numPerPage);
        emailSyncQuery.next({
            anchor_id,
            mailbox_id: localProps.query.mailboxId,
            sorts: [{ column: 'Date', asc: false }],
            limit: (maxPage - minPage + 1) * numPerPage * 10,
        })
    });

    return <LazyLoadingList
        {...listProps}
        numPerPage={numPerPage}
        watchPage={watchPage}
        pages={pageStore}
        watchingPages={watchingPages}>
        {(thread) => (
            <div>{thread?.emails?.at(0)?.subject}</div>
        )}
    </LazyLoadingList>;
}

function last<T>(arr: T[] | undefined): T | undefined {
    if (!arr || arr.length === 0) {
        return undefined;
    }

    return arr[arr.length - 1];
}