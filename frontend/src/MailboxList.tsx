import {For, JSX, Match, Show, splitProps} from "solid-js";
import {streamWebSocketApi} from "./streamApi";
import {createSignalFromObservableNoError} from "./observables";
import * as zod from "zod";
import InboxIcon from "heroicons/24/outline/inbox.svg?raw";
import PaperPlaneIcon from "heroicons/24/outline/paper-airplane.svg?raw";
import FolderIcon from "heroicons/24/outline/folder.svg?raw"
import TrashIcon from "heroicons/24/outline/trash.svg?raw";
import FileIcon from "heroicons/24/outline/document.svg?raw";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

const MailboxSchema = zod.object({
    id: zod.string(),
    name: zod.string(),
    role: zod.enum(["inbox", "sent", "drafts", "sent", "junk", "trash", "templates", "outbox", "scheduled"])
        .nullable().optional(),
    sortOrder: zod.number(),
    totalEmails: zod.number(),
    parentId: zod.string().nullable().optional(),
})

type Mailbox = zod.infer<typeof MailboxSchema>;

type Props = {
    accountId: string;
    selectedMailboxId?: string;
    onMailboxSelected?: (mailboxId: string) => void;
} & JSX.HTMLAttributes<HTMLUListElement>;

export default function MailboxList(props: Props) {
    const [localProps, listProps] = splitProps(props, ["accountId"]);

    const mailboxes = createSignalFromObservableNoError(() => streamWebSocketApi(
        `${apiUrl}/mailboxes/${localProps.accountId}`,
        zod.array(MailboxSchema),
    ), {
        state: "connecting",
    });

    const sortedMailboxes = () => {
        if (mailboxes().lastValue) {
            return collapseMailboxes(mailboxes().lastValue!, null);
        }

        return null;
    };

    const onItemClick = props.onMailboxSelected ? (evt: Event) => {
        const ele = evt.currentTarget as HTMLElement;
        props.onMailboxSelected?.(ele.dataset.id as string);
    } : undefined;

    return <Show when={sortedMailboxes()} fallback={<p>Loading...</p>}>
        <ul {...listProps}>
            <For each={sortedMailboxes()}>
                {(mailbox) => <MailboxItem
                    mailbox={mailbox}
                    onItemClick={onItemClick}
                    selectedMailboxId={props.selectedMailboxId}/>}
            </For>
        </ul>
    </Show>
}

function MailboxIcon(props: { role?: Mailbox["role"], class?: string }) {
    let icon;
    switch (props.role) {
        case "inbox":
            icon = InboxIcon;
            break;
        case "junk":
            icon = TrashIcon;
            break;
        case "sent":
            icon = PaperPlaneIcon;
            break;
        case "drafts":
            icon = FileIcon;
            break;
        default:
            icon = FolderIcon;
            break;
    }

    return <span class="size-4" innerHTML={icon}></span>;
}

function MailboxItem(props: {
    mailbox: CollapsedMailbox,
    onItemClick?: (evt: Event) => void,
    selectedMailboxId?: string
}) {
    return <Show
        when={props.mailbox.children && props.mailbox.children.length > 0}
        fallback={<li>
            <a
                onClick={props.onItemClick}
                data-id={props.mailbox.id}
                class={props.mailbox.id === props.selectedMailboxId ? "menu-active w-full" : "w-full"}>
                <MailboxIcon role={props.mailbox.role}/>
                {props.mailbox.name}
            </a>
        </li>}>
        <li>
            <details open>
                <summary>
                    <MailboxIcon role={props.mailbox.role}/>
                    {props.mailbox.name}
                </summary>

                <ul>
                    <For each={props.mailbox.children}>
                        {(childMailbox) => <MailboxItem
                            mailbox={childMailbox}
                            onItemClick={props.onItemClick}
                            selectedMailboxId={props.selectedMailboxId}/>}
                    </For>
                </ul>
            </details>
        </li>

    </Show>;
}

export type CollapsedMailbox = {
    children?: CollapsedMailbox[];
} & Mailbox;

function collapseMailboxes(mailboxes: Mailbox[], parentId: string | null): CollapsedMailbox[] {
    const currentLevel = (mailboxes.filter((mb) => mb.parentId === parentId)
        .toSorted((a, b) => a.sortOrder - b.sortOrder)) as CollapsedMailbox[];

    for (const mb of currentLevel) {
        mb.children = collapseMailboxes(mailboxes, mb.id);
    }

    return currentLevel;
}