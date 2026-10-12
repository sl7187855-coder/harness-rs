import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const API = process.env.THOMAS_MOBILE_API || "http://127.0.0.1:3195";
const API_CANDIDATES = [...new Set([API, "http://127.0.0.1:3195", "http://127.0.0.1:3198"].filter(Boolean))];

async function api(path, init = {}, base = API) {
  const res = await fetch(base + path, init);
  const text = await res.text();
  let data = {};
  try { data = text ? JSON.parse(text) : {}; } catch { data = { raw: text }; }
  if (!res.ok) throw new Error(data.error || data.raw || "mobile bridge request failed");
  return data;
}
async function devices(){
  const merged=new Map();
  let responded=false,lastError=null;
  for(const base of API_CANDIDATES){
    try{
      const d=await api("/devices",{},base);
      responded=true;
      for(const item of (Array.isArray(d.devices)?d.devices:[])){
        const id=String(item?.reportedDeviceId||item?.deviceId||"").trim();
        if(id&&!merged.has(id))merged.set(id,item);
      }
    }catch(e){lastError=e}
  }
  if(!responded&&lastError)throw lastError;
  return {devices:[...merged.values()]};
}
async function resolveDevice(deviceId){
  const id=String(deviceId||"").trim();
  if(id)return id;
  const d=await devices();
  if(!d.devices.length)throw new Error("Nenhum celular Thomas está conectado.");
  if(d.devices.length>1)throw new Error("Há mais de um celular conectado; informe deviceId.");
  return String(d.devices[0].reportedDeviceId||d.devices[0].deviceId);
}
async function apiForDevice(deviceId){
  const wanted=String(deviceId||"").trim();
  let lastError=null;
  for(const base of API_CANDIDATES){
    try{
      const d=await api("/devices",{},base);
      const list=Array.isArray(d.devices)?d.devices:[];
      if(list.some(item=>String(item?.reportedDeviceId||item?.deviceId||"").trim()===wanted))return base;
    }catch(e){lastError=e}
  }
  throw lastError||new Error("Celular não encontrado no Thomas Mobile.");
}
async function command(action,deviceId,params={}){
  const id=await resolveDevice(deviceId);
  const base=await apiForDevice(id);
  return await api("/command",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({deviceId:id,action,params})},base);
}
async function commandAll(action,deviceIds,params={}){
  const listed=await devices();
  const ids=Array.isArray(deviceIds)&&deviceIds.length?deviceIds.map(String):listed.devices.map(x=>String(x.reportedDeviceId||x.deviceId));
  const unique=[...new Set(ids.filter(Boolean))];
  const results=await Promise.all(unique.map(async deviceId=>{
    try{
      const base=await apiForDevice(deviceId);
      const value=await api("/command",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({deviceId,action,params})},base);
      return {deviceId,ok:true,result:value.result??value};
    }catch(e){
      return {deviceId,ok:false,error:e.message};
    }
  }));
  return {ok:results.every(x=>x.ok),action:String(action),count:results.length,results};
}
function out(value){ return {content:[{type:"text",text:JSON.stringify(value)}],structuredContent:value}; }
const optDevice={deviceId:z.string().optional()};
const server=new McpServer({name:"thomas-mobile",version:"0.5.3"},{instructions:"Authorized Android control. Prefer accessibility-aware actions and verify important actions."});

server.registerTool("mobile_devices",{description:"List connected Android phones paired with Thomas Mobile Agent.",inputSchema:{}},async()=>out(await devices()));
server.registerTool("mobile_observe",{description:"Inspect current Android accessibility tree and active package.",inputSchema:optDevice},async({deviceId})=>out((await command("observe",deviceId)).result));
server.registerTool("mobile_tap",{description:"Tap an exact Android screen coordinate.",inputSchema:{deviceId:z.string().optional(),x:z.number(),y:z.number()}},async({deviceId,x,y})=>out((await command("tap",deviceId,{x,y})).result));
server.registerTool("mobile_long_press",{description:"Long press an Android screen coordinate.",inputSchema:{deviceId:z.string().optional(),x:z.number(),y:z.number(),durationMs:z.number().int().optional()}},async({deviceId,x,y,durationMs})=>out((await command("long_press",deviceId,{x,y,durationMs})).result));
server.registerTool("mobile_swipe",{description:"Swipe between two Android coordinates.",inputSchema:{deviceId:z.string().optional(),x1:z.number(),y1:z.number(),x2:z.number(),y2:z.number(),durationMs:z.number().int().optional()}},async({deviceId,x1,y1,x2,y2,durationMs})=>out((await command("swipe",deviceId,{x1,y1,x2,y2,durationMs})).result));
server.registerTool("mobile_scroll",{description:"Scroll the active Android scrollable container.",inputSchema:{deviceId:z.string().optional(),direction:z.enum(["forward","backward"]).optional()}},async({deviceId,direction})=>out((await command("scroll",deviceId,{direction:direction||"forward"})).result));
server.registerTool("mobile_type",{description:"Type text into the focused Android editable field.",inputSchema:{deviceId:z.string().optional(),text:z.string()}},async({deviceId,text})=>out((await command("type",deviceId,{text})).result));
server.registerTool("mobile_clear_text",{description:"Clear the focused Android editable field.",inputSchema:optDevice},async({deviceId})=>out((await command("clear_text",deviceId)).result));
server.registerTool("mobile_tap_text",{description:"Find visible Android accessibility text containing the query and activate it.",inputSchema:{deviceId:z.string().optional(),query:z.string()}},async({deviceId,query})=>out((await command("tap_text",deviceId,{query})).result));
server.registerTool("mobile_tap_description",{description:"Find visible Android content description containing the query and activate it.",inputSchema:{deviceId:z.string().optional(),query:z.string()}},async({deviceId,query})=>out((await command("tap_description",deviceId,{query})).result));
server.registerTool("mobile_find_text",{description:"Find visible Android accessibility matches without touching the phone.",inputSchema:{deviceId:z.string().optional(),query:z.string(),field:z.enum(["text","description","resourceId"]).optional()}},async({deviceId,query,field})=>out((await command("find_text",deviceId,{query,field:field||"text"})).result));
server.registerTool("mobile_wait_for_text",{description:"Wait for Android accessibility text/description/resourceId to appear.",inputSchema:{deviceId:z.string().optional(),query:z.string(),field:z.enum(["text","description","resourceId"]).optional(),timeoutMs:z.number().int().optional()}},async({deviceId,query,field,timeoutMs})=>out((await command("wait_for_text",deviceId,{query,field:field||"text",timeoutMs})).result));
for(const [name,action] of [["mobile_back","back"],["mobile_home","home"],["mobile_recents","recents"]]){
  server.registerTool(name,{description:"Perform Android "+action+" global action.",inputSchema:optDevice},async({deviceId})=>out((await command(action,deviceId)).result));
}
const controlDevice = {deviceId:z.string().optional()};
server.registerTool("mobile_notifications",{description:"Open the Android notification shade on the authorized phone.",inputSchema:controlDevice},async({deviceId})=>out((await command("notifications",deviceId)).result));
server.registerTool("mobile_quick_settings",{description:"Open Android Quick Settings on the authorized phone.",inputSchema:controlDevice},async({deviceId})=>out((await command("quick_settings",deviceId)).result));
server.registerTool("mobile_power_menu",{description:"Open the Android power menu. Use this instead of pretending an app can physically power the device on.",inputSchema:controlDevice},async({deviceId})=>out((await command("power_menu",deviceId)).result));
server.registerTool("mobile_lock",{description:"Lock the authorized Android device.",inputSchema:controlDevice},async({deviceId})=>out((await command("lock",deviceId)).result));
server.registerTool("mobile_wake",{description:"Wake the authorized Android screen.",inputSchema:controlDevice},async({deviceId})=>out((await command("wake",deviceId)).result));
server.registerTool("mobile_unlock",{description:"Start the Android device-authentication screen when the device is locked. Does not bypass PIN/password/biometric security.",inputSchema:controlDevice},async({deviceId})=>out((await command("unlock",deviceId)).result));
server.registerTool("mobile_open_settings",{description:"Open Android settings or a specific settings panel such as wifi, bluetooth, accessibility, or security.",inputSchema:{deviceId:z.string().optional(),kind:z.enum(["settings","wifi","bluetooth","accessibility","security"]).optional()}},async({deviceId,kind})=>out((await command("open_settings",deviceId,{kind:kind||"settings"})).result));
server.registerTool("mobile_brightness",{description:"Read/set Android screen brightness. Use value 0-255 or percent 0-100.",inputSchema:{deviceId:z.string().optional(),value:z.number().int().min(0).max(255).optional(),percent:z.number().min(0).max(100).optional()}},async({deviceId,value,percent})=>out((await command("brightness",deviceId,{value,percent})).result));
server.registerTool("mobile_volume",{description:"Adjust Android stream volume: up, down, mute, unmute, or set an absolute value.",inputSchema:{deviceId:z.string().optional(),direction:z.enum(["up","down","mute","unmute"]).optional(),value:z.number().int().min(0).optional(),stream:z.number().int().optional()}},async({deviceId,direction,value,stream})=>out((await command("volume",deviceId,{direction,value,stream})).result));
server.registerTool("mobile_screen_state",{description:"Get whether the Android screen is interactive and whether the device is keyguard-locked.",inputSchema:controlDevice},async({deviceId})=>out((await command("screen_state",deviceId)).result));
server.registerTool("mobile_battery",{description:"Get the Android battery percentage and charging state.",inputSchema:controlDevice},async({deviceId})=>out((await command("battery",deviceId)).result));
server.registerTool("mobile_device_info",{description:"Get Android model, OS, SDK, display size, and locale.",inputSchema:controlDevice},async({deviceId})=>out((await command("device_info",deviceId)).result));
server.registerTool("mobile_reboot",{description:"Reboot the authorized Android device when Thomas Mobile is configured as Device Owner. Otherwise returns the required privilege instead of bypassing Android security.",inputSchema:controlDevice},async({deviceId})=>out((await command("reboot",deviceId)).result));
server.registerTool("mobile_command_all",{description:"Run one supported action across all connected Android phones, or a selected list of deviceIds. Use only for actions appropriate for every target.",inputSchema:{action:z.enum(["observe","home","back","recents","notifications","quick_settings","power_menu","lock","wake","unlock","screen_state","battery","device_info"]),deviceIds:z.array(z.string()).optional()}},async({action,deviceIds})=>out(await commandAll(action,deviceIds||[],{})));
server.registerTool("mobile_launch",{description:"Launch an Android app by package name.",inputSchema:{deviceId:z.string().optional(),packageName:z.string()}},async({deviceId,packageName})=>out((await command("launch",deviceId,{packageName})).result));
server.registerTool("mobile_open_url",{description:"Open a URL on the authorized Android phone.",inputSchema:{deviceId:z.string().optional(),url:z.string()}},async({deviceId,url})=>out((await command("open_url",deviceId,{url})).result));
server.registerTool("mobile_clipboard_set",{description:"Set the Android system clipboard.",inputSchema:{deviceId:z.string().optional(),text:z.string()}},async({deviceId,text})=>out((await command("clipboard_set",deviceId,{text})).result));
server.registerTool("mobile_clipboard_get",{description:"Read the Android system clipboard.",inputSchema:optDevice},async({deviceId})=>out((await command("clipboard_get",deviceId)).result));
server.registerTool("mobile_screenshot",{description:"Capture the current Android screen for visual reasoning.",inputSchema:optDevice},async({deviceId})=>{
  const response=await command("screenshot",deviceId),value=response.result||response,data=String(value.data||"");
  if(!data) throw new Error("O celular não retornou screenshot.");
  return {content:[{type:"text",text:"Tela do celular: "+(value.width||"?")+"x"+(value.height||"?")+" px."},{type:"image",data,mimeType:value.mime||"image/jpeg"}],structuredContent:{deviceId:await resolveDevice(deviceId),width:value.width,height:value.height,mime:value.mime||"image/jpeg",bytes:Buffer.byteLength(data,"base64")}};
});
await server.connect(new StdioServerTransport());
