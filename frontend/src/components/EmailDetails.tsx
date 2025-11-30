import {AccountId, BodyPart, Email} from "./ThreadList";
import {createMemo, createResource, createSignal, Match, Show, Switch} from "solid-js";
import * as zod from "zod";
import {log as parentLog} from "../log";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

const log = parentLog.child({ "component": "EmailDetails" });

export default function EmailDetails(props: {
    accountId: AccountId,
    email: Email,
}) {
    const getPartUrl = (search: (part: BodyPart) => boolean) => {
        const part = props.email.htmlBody.find(search) || props.email.textBody.find(search);
        if (part) {
            const mimeType = (!part.type.startsWith("text/") || part.type.includes("charset")) ? part.type : `${part.type}; charset=utf-8`;
            return `${apiUrl}/blobs/${props.accountId}/${part.blobId}?mimeType=${encodeURIComponent(mimeType)}`;
        }
    };

    function replaceCidLink<K extends keyof HTMLElementTagNameMap>(
        document: HTMLDocument,
        selector: K,
        attribute: string) {
        document.querySelectorAll(selector).forEach((element) => {
            const link = element.getAttribute(attribute);
            if (link && link.startsWith("cid:")) {
                log.info({"link": link}, 'Replacing cid link');
                const cid = link.substring(4);
                const realLink = getPartUrl((part) => part.cid === cid);
                if (realLink) {
                    element.setAttribute(attribute, realLink);
                } else {
                    element.removeAttribute(attribute);
                }
            }
        });
    }


    const mainHtmlBodyUrl = createMemo(() => {
        return getPartUrl((part) => part.type.startsWith("text/html"));
    });

    const mainTextBodyUrl = createMemo(() => {
        return getPartUrl((part) => part.type.startsWith("text/plain"));
    });

    const onPageLoaded = (event: Event) => {
        const iframe = event.target as HTMLIFrameElement;
        console.log(iframe);
        replaceCidLink(iframe.contentDocument!, 'img', 'src');
        replaceCidLink(iframe.contentDocument!, 'link', 'href');

        iframe.contentDocument!.querySelectorAll('a').forEach((link) => {
            link.target = "_blank";
        });
    };

    log.debug({email: props.email}, 'Rendering')

    return <>
        <Switch fallback={"No text available"}>
            <Match when={!!mainHtmlBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll"
                        src={mainHtmlBodyUrl()}
                        onload={onPageLoaded}
                        sandbox="allow-popups"/>
            </Match>

            <Match when={!!mainTextBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll" src={mainTextBodyUrl()} sandbox=""/>
            </Match>
        </Switch>
    </>
}