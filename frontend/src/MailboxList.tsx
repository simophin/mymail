import {For, JSX, splitProps} from "solid-js";
import {createWebSocketResource} from "./streamApi";

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

    const mailboxes = createWebSocketResource<Mailbox[]>([], () => new WebSocket(`${apiUrl}/mailboxes/${localProps.accountId}`));

    const onItemClick = props.onMailboxSelected ? (evt: Event) => {
        const ele = evt.currentTarget as HTMLElement;
        props.onMailboxSelected?.(ele.dataset.id as string);
    } : undefined;

    return <ul {...listProps}>
        <For each={mailboxes()}>
            {(mailbox) => <li
                onClick={onItemClick}
                data-id={mailbox.id}
            >{mailbox.name}</li>}
        </For>
    </ul>;
}