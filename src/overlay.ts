import { mount } from "svelte";
import Overlay from "./Overlay.svelte";

const overlay = mount(Overlay, { target: document.getElementById("app")! });

export default overlay;
