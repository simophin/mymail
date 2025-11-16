import {For, JSX, Show, splitProps} from "solid-js";
import {createWebSocketResource, streamWebSocketApi} from "./streamApi";
import {createSignalFromObservableNoError} from "./observables";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

type Mailbox = {
    id: string,
    name: string,
}

type Props = {
    accountId: string;
    selectedMailboxId?: string;
    onMailboxSelected?: (mailboxId: string) => void;
} & JSX.HTMLAttributes<HTMLUListElement>;

export default function MailboxList(props: Props) {
    const [localProps, listProps] = splitProps(props, ["accountId"]);

    const mailboxes = createSignalFromObservableNoError(() => streamWebSocketApi<Mailbox[]>(
        `${apiUrl}/mailboxes/${localProps.accountId}`
    ), {
        state: "connecting",
    });

    const onItemClick = props.onMailboxSelected ? (evt: Event) => {
        const ele = evt.currentTarget as HTMLElement;
        props.onMailboxSelected?.(ele.dataset.id as string);
    } : undefined;

    return <Show when={mailboxes().lastValue} fallback={<p>Loading...</p>}>
        <ul {...listProps}>
            <For each={mailboxes().lastValue!}>
                {(mailbox) => <li><a
                    onClick={onItemClick}
                    data-id={mailbox.id}
                    class={mailbox.id === props.selectedMailboxId ? "menu-active w-full" : "w-full"}>{mailbox.name}</a></li>}
            </For>
        </ul>
    </Show>
}