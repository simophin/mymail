import {Thread} from "./ThreadList";
import {For} from "solid-js";
import EmailDetails from "./EmailDetails";

export default function ThreadDetails(props: {
    thread: Thread,
    class?: string,
}) {
    return <div class={props.class}>
        <For each={props.thread.emails}>
            {(email) => <EmailDetails email={email}/>}
        </For>
    </div>;
}