import {AccountId, Email} from "./ThreadList";
import {createMemo, createResource, Match, Switch} from "solid-js";
import * as zod from "zod";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

export default function EmailDetails(props: {
    accountId: AccountId,
    email: Email,
}) {
    const mainHtmlBodyUrl = createMemo(() => {
        const firstHtml = props.email.htmlBody.find((part) => part.type === "text/html");
        if (firstHtml) {
            return `${apiUrl}/blobs/${props.accountId}/${firstHtml.blobId}?mimeType=${encodeURIComponent(firstHtml.type)}&blockImages=true`;
        }
    });

    const mainTextBodyUrl = createMemo(() => {

    });

   return <Switch fallback={"No text available"}>
       <Match when={!!mainHtmlBodyUrl()}>
           <iframe class="w-full h-full overflow-scroll" src={mainHtmlBodyUrl()} sandbox="" />
       </Match>
   </Switch>
}