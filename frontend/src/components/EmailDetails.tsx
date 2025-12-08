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
    const getPartUrl = (parts: BodyPart[], search: (part: BodyPart) => boolean) => {
        const part = parts.find(search);
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
                const cid = link.substring(4);
                const realLink = getPartUrl(props.email.attachments ?? [], (part) => part.cid === cid);
                if (realLink) {
                    element.setAttribute(attribute, realLink);
                } else {
                    element.removeAttribute(attribute);
                }
            }
        });
    }


    const mainHtmlBodyUrl = createMemo(() => {
        return getPartUrl(props.email.htmlBody, (part) => part.type.startsWith("text/html"));
    });

    const mainTextBodyUrl = createMemo(() => {
        return getPartUrl(props.email.textBody, (part) => part.type.startsWith("text/plain"));
    });

    const onPageLoaded = (event: Event) => {
        const iframe = event.target as HTMLIFrameElement;

        iframe.contentDocument?.querySelectorAll('img').forEach(img => {
            const src = img.src;
            if (src && src.startsWith('cid:')) {
                const cid = src.substring(4);
                const realLink = getPartUrl(props.email.attachments ?? [], (part) => part.cid === cid);
                if (realLink) {
                    img.src = realLink;
                } else {
                    img.removeAttribute('src');
                }
            } else if (src) {
                img.src = `${apiUrl}/proxy-image?url=${encodeURIComponent(src)}`;
            }
        });

        iframe.contentDocument?.querySelectorAll('a').forEach((link) => {
            link.target = "_blank";
        });
    };

    return <>
        <Switch fallback={"No text available"}>
            <Match when={!!mainHtmlBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll"
                        src={mainHtmlBodyUrl()}
                        onload={onPageLoaded}
                        sandbox="allow-popups allow-same-origin"/>
            </Match>

            <Match when={!!mainTextBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll" src={mainTextBodyUrl()} sandbox=""/>
            </Match>
        </Switch>
    </>
}