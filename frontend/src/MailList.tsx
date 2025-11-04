import { Subscription } from "rxjs";
import { createEffect, createSignal, Index } from "solid-js";
import { createStore } from "solid-js/store/types/server.js";
const apiUrl = import.meta.env.BASE_URL;

export type EmailSort = {
    column: string;
    asc: boolean;
}


export type BasicEmailDbQuery = {
    mailboxId?: string;
    searchKeyword?: string;
}

export type EmailDbQuery = BasicEmailDbQuery & {
    sorts: EmailSort[];
    limit: number;
    offset: number;
}

export type AccountId = number;

export type Email = {
    id: string;
    subject: string;
}

export type Thread = Email[];

type PageWatchState = {
    subscription: Subscription;
}

export default function MailList(props: {
    accountId: AccountId;
    query: BasicEmailDbQuery
}) {
    const [pages, setPages] = createStore<Thread[][]>([]);
    const [watchState, setWatchState] = createStore<{ [page: number]: PageWatchState }>({});
}