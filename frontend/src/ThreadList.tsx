import { Subscription } from "rxjs";
import { createEffect, createSignal, Index } from "solid-js";
import { createStore } from "solid-js/store/types/server.js";
const apiUrl = import.meta.env.BASE_URL;

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
}

export type Thread = {
    id: string;
    emails: Email[];
}

type PageWatchState = {
    subscription: Subscription;
}

export default function ThreadList(props: {
    query: ThreadQuery,
    numPerPage: number,
}) {
    const [pages, setPages] = createStore<Thread[][]>([]);
    const [watchState, setWatchState] = createStore<{ [page: number]: PageWatchState }>({});
}