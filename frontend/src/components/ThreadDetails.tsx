import {AccountId, Thread} from "./ThreadList";
import {For} from "solid-js";
import EmailDetails from "./EmailDetails";

export default function ThreadDetails(props: {
    accountId: AccountId,
    thread: Thread,
    class?: string,
}) {
    return <div class={props.class}>
        <For each={props.thread.emails}>
            {(email) => <EmailDetails accountId={props.accountId} email={email}/>}
        </For>
    </div>;
}