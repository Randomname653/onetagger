<template>
<div class='id-wrapper'>
<div class='id-content'>

    <!-- Setup -->
    <div class='text-subtitle2 text-bold text-primary q-mt-lg'>IDENTIFY</div>
    <div class='text-subtitle2 text-grey-6 q-mb-md'>
        Zero Trust: Eine Datei gilt nur als erkannt, wenn AcoustID (Fingerprint) und Shazam übereinstimmen –
        vorhandene Tags und Dateinamen zählen nicht.
    </div>

    <div class='row q-col-gutter-md'>
        <q-input filled dense class='col-6' label='Quellordner' v-model='config.path' :disable='running'>
            <template v-slot:append>
                <q-btn round dense flat icon='mdi-open-in-app' @click='$1t.browse("id", config.path)'></q-btn>
            </template>
        </q-input>
        <q-input filled dense class='col-6' label='Zielordner (getaggte Dateien werden hierhin verschoben)' v-model='config.target' :disable='running'>
            <template v-slot:append>
                <q-btn round dense flat icon='mdi-open-in-app' @click='$1t.browse("idTarget", config.target)'></q-btn>
            </template>
        </q-input>
        <q-input filled dense class='col-6' label='Ordner-/Dateivorlage' v-model='config.template' :disable='running'
            hint='%albumartist% %artist% %album% %year% %track% %title%'></q-input>
        <q-input filled dense class='col-3' label='AcoustID API-Key' v-model='config.acoustidKey' :type='showKey ? "text" : "password"' :disable='running'>
            <template v-slot:append>
                <q-icon :name='showKey ? "mdi-eye-off" : "mdi-eye"' class='cursor-pointer' @click='showKey = !showKey'></q-icon>
            </template>
        </q-input>
        <div class='col-3'>
            <div class='text-caption text-grey-6'>Threads: {{ config.threads }}</div>
            <q-slider v-model='config.threads' :min='1' :max='8' :disable='running' label></q-slider>
        </div>
    </div>
    <div class='row items-center q-mt-sm q-gutter-md'>
        <q-toggle v-model='config.includeSubfolders' label='Unterordner einbeziehen' :disable='running'></q-toggle>
        <q-toggle v-model='config.autoApply' label='Bestätigte automatisch taggen & verschieben' :disable='running'></q-toggle>
        <q-space></q-space>
        <q-btn v-if='!running' color='primary' class='text-black' icon='mdi-play' label='Start' @click='start'
            :disable='!config.path || !config.acoustidKey || (config.autoApply && !config.target)'></q-btn>
        <q-btn v-if='running' color='red' icon='mdi-stop' label='Stop' @click='stop'></q-btn>
        <q-btn v-if='!running' flat icon='mdi-history' label='Letzten Lauf laden' @click='loadLast'></q-btn>
    </div>

    <!-- Progress -->
    <div v-if='total > 0' class='q-mt-lg'>
        <q-linear-progress :value='total ? results.length / total : 0' color='primary' size='8px' rounded></q-linear-progress>
        <div class='row items-center q-mt-sm q-gutter-sm'>
            <div class='text-caption text-grey-5'>{{ results.length }} / {{ total }} <span v-if='running && eta'>– noch ca. {{ eta }}</span></div>
            <q-space></q-space>
            <q-chip clickable :outline='filter != "all"' color='grey-8' @click='filter = "all"'>Alle {{ results.length }}</q-chip>
            <q-chip clickable :outline='filter != "confirmed"' color='green-8' text-color='white' @click='filter = "confirmed"'>
                Bestätigt {{ count("confirmed") }} <span v-if='appliedCount'>&nbsp;({{ appliedCount }} angewendet)</span>
            </q-chip>
            <q-chip clickable :outline='filter != "review"' color='orange-8' text-color='white' @click='filter = "review"'>Prüfen {{ count("review") }}</q-chip>
            <q-chip clickable :outline='filter != "unknown"' color='grey-7' text-color='white' @click='filter = "unknown"'>Unbekannt {{ count("unknown") }}</q-chip>
            <q-chip clickable :outline='filter != "error"' color='red-8' text-color='white' @click='filter = "error"'>Fehler {{ count("error") }}</q-chip>
        </div>
    </div>

    <!-- Results -->
    <q-virtual-scroll v-if='results.length' :items='filtered' style='max-height: calc(100vh - 430px)' class='q-mt-sm id-list' separator>
        <template v-slot='{ item }'>
            <q-item :key='item.index' clickable @click='open(item.index)' dense>
                <q-item-section avatar>
                    <q-icon :name='icon(item.r)' :color='color(item.r)'></q-icon>
                </q-item-section>
                <q-item-section>
                    <q-item-label class='text-grey-4 ellipsis'>{{ filename(item.r.path) }}</q-item-label>
                    <q-item-label caption class='ellipsis'>{{ item.r.reason }}</q-item-label>
                </q-item-section>
                <q-item-section side v-if='item.r.applied'>
                    <q-icon name='mdi-folder-check' color='green-5'><q-tooltip>{{ item.r.applied }}</q-tooltip></q-icon>
                </q-item-section>
            </q-item>
        </template>
    </q-virtual-scroll>

    <!-- History / Undo -->
    <div class='q-mt-lg q-mb-xl'>
        <q-btn flat dense icon='mdi-undo-variant' label='Verlauf / Rückgängig' @click='loadJournals' :disable='!config.target'></q-btn>
        <q-list v-if='journals.length' dense class='q-mt-sm'>
            <q-item v-for='j in journals' :key='j.path'>
                <q-item-section>
                    <q-item-label>{{ j.name }}</q-item-label>
                    <q-item-label caption>{{ j.files }} Dateien angewendet</q-item-label>
                </q-item-section>
                <q-item-section side>
                    <q-btn flat dense color='red-4' icon='mdi-undo' label='Rückgängig' :disable='running || !j.files' @click='undo(j)'></q-btn>
                </q-item-section>
            </q-item>
        </q-list>
    </div>
</div>

<!-- Review dialog -->
<q-dialog v-model='dialog'>
    <q-card v-if='current' style='min-width: 900px; max-width: 1100px'>
        <q-card-section>
            <div class='row items-center'>
                <q-btn round flat :icon='$1t.player.value.playing ? "mdi-pause" : "mdi-play"' color='primary' @click='play'></q-btn>
                <div class='text-subtitle1 text-bold q-ml-sm ellipsis' style='max-width: 800px'>{{ filename(current.path) }}</div>
            </div>
            <div class='text-caption text-grey-5 q-mt-xs'>{{ current.reason }}</div>
        </q-card-section>

        <q-card-section class='row q-col-gutter-md q-pt-none'>
            <!-- Shazam -->
            <div class='col-4'>
                <div class='text-overline text-primary'>Shazam</div>
                <div v-if='current.shazam'>
                    <div class='text-bold'>{{ current.shazam.artist }} – {{ current.shazam.title }}</div>
                    <div class='text-caption'>Album: {{ current.shazam.album ?? '-' }}</div>
                    <div class='text-caption'>Jahr: {{ current.shazam.released ?? '-' }} · ISRC: {{ current.shazam.isrc ?? '-' }}</div>
                </div>
                <div v-else class='text-grey-6'>kein Treffer</div>
            </div>
            <!-- AcoustID -->
            <div class='col-5'>
                <div class='text-overline text-primary'>AcoustID</div>
                <div v-if='!current.recordings.length' class='text-grey-6'>kein Treffer</div>
                <div v-for='rec in current.recordings.slice(0, 4)' :key='rec.id' class='text-caption'>
                    <q-icon :name='rec.shazamAgrees ? "mdi-check" : "mdi-close"' :color='rec.shazamAgrees ? "green" : "grey"'></q-icon>
                    <b>{{ rec.artist }} – {{ rec.title }}</b> ({{ rec.score.toFixed(2) }}<span v-if='!rec.durationOk'>, Dauer passt nicht</span>)
                    <div class='text-grey-6 q-ml-md'>{{ rec.note }}</div>
                </div>
            </div>
            <!-- Claims -->
            <div class='col-3'>
                <div class='text-overline text-grey-6'>Behauptet (Tags)</div>
                <div class='text-caption text-grey-6'>{{ current.claims.artist ?? '-' }} – {{ current.claims.title ?? '-' }}</div>
                <div class='text-caption text-grey-6'>{{ current.claims.album ?? '-' }}</div>
            </div>
        </q-card-section>

        <!-- Releases -->
        <q-card-section class='q-pt-none'>
            <div class='text-overline text-primary'>Release</div>
            <div v-if='!current.releases.length' class='text-grey-6'>Keine Releases – nur überspringen möglich.</div>
            <q-scroll-area v-else style='height: 260px'>
                <q-list dense>
                    <q-item v-for='(rel, i) in current.releases' :key='rel.releaseId + i' tag='label'>
                        <q-item-section avatar>
                            <q-radio v-model='selectedRelease' :val='i'></q-radio>
                        </q-item-section>
                        <q-item-section>
                            <q-item-label>
                                <b>{{ rel.title }}</b> – {{ rel.artist }}
                                <span class='text-grey-6'>({{ [rel.kind, ...rel.secondary].filter(x => x).join(', ') }}, {{ rel.date ?? '?' }}<span v-if='rel.track'>, Track {{ rel.disc }}-{{ rel.track }}</span>)</span>
                            </q-item-label>
                            <q-item-label caption>{{ rel.reasons.join(' · ') }}</q-item-label>
                        </q-item-section>
                        <q-item-section side>{{ rel.score }}</q-item-section>
                    </q-item>
                </q-list>
            </q-scroll-area>
        </q-card-section>

        <q-card-actions align='right'>
            <q-btn flat label='Überspringen' @click='skip'></q-btn>
            <q-btn flat label='Schließen' v-close-popup></q-btn>
            <q-btn color='primary' class='text-black' label='Übernehmen' icon='mdi-check'
                :disable='!current.releases.length || !!current.applied || !config.target' :loading='applying' @click='applySelected'></q-btn>
        </q-card-actions>
    </q-card>
</q-dialog>
</div>
</template>

<script lang='ts' setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useQuasar } from 'quasar';
import { get1t } from '../scripts/onetagger';

const $1t = get1t();
const $q = useQuasar();

class IdentifyConfig {
    path: string = '';
    target: string = '';
    template: string = '%albumartist%/%album% (%year%)/%track% - %title%';
    acoustidKey: string = '';
    threads: number = 4;
    includeSubfolders: boolean = true;
    autoApply: boolean = true;
}

const config = ref(new IdentifyConfig());
const results = ref<any[]>([]);
const total = ref(0);
const running = ref(false);
const filter = ref('all');
const showKey = ref(false);
const dialog = ref(false);
const currentIndex = ref(-1);
const selectedRelease = ref(0);
const applying = ref(false);
const journals = ref<any[]>([]);
const started = ref(0);
let timer: any = null;

const current = computed(() => currentIndex.value >= 0 ? results.value[currentIndex.value] : null);
const filtered = computed(() => results.value.map((r, index) => ({ r, index }))
    .filter(x => filter.value == 'all' || x.r.state == filter.value).reverse());
const appliedCount = computed(() => results.value.filter(r => r.applied).length);
const eta = computed(() => {
    if (!started.value || results.value.length < 5) return '';
    const per = (Date.now() - started.value) / results.value.length;
    const ms = per * (total.value - results.value.length);
    const h = Math.floor(ms / 3600000), m = Math.round((ms % 3600000) / 60000);
    return h > 0 ? `${h} h ${m} min` : `${m} min`;
});

function count(state: string) {
    return results.value.filter(r => r.state == state).length;
}
function filename(p: string) {
    return p.split(/[\\/]/).pop();
}
function icon(r: any) {
    return { confirmed: 'mdi-check-circle', review: 'mdi-alert-circle', unknown: 'mdi-help-circle', error: 'mdi-close-circle' }[r.state as string] ?? 'mdi-circle';
}
function color(r: any) {
    return { confirmed: 'green-5', review: 'orange-5', unknown: 'grey-6', error: 'red-5' }[r.state as string] ?? 'grey';
}

function saveConfig() {
    $1t.settings.value.identify = JSON.parse(JSON.stringify(config.value));
    $1t.saveSettings(false);
}
function uiConfig() {
    return JSON.parse(JSON.stringify(config.value));
}

function start() {
    saveConfig();
    results.value = [];
    total.value = 0;
    started.value = Date.now();
    $1t.send('identifyStart', { config: uiConfig() });
}
function stop() {
    $1t.send('identifyStop');
}
function loadLast() {
    $1t.send('identifyLoadLast');
}
function poll() {
    $1t.send('identifyStatus', { since: results.value.length });
}
function startPolling() {
    if (!timer) timer = setInterval(poll, 1500);
}

function open(index: number) {
    currentIndex.value = index;
    selectedRelease.value = results.value[index].release ?? 0;
    dialog.value = true;
}
function play() {
    const p = $1t.player.value;
    if (p.path != current.value.path) {
        p.loadTrack(current.value.path);
        setTimeout(() => p.play(), 300);
    } else if (p.playing) {
        p.pause();
    } else {
        p.play();
    }
}
function applySelected() {
    applying.value = true;
    $1t.send('identifyApply', { index: currentIndex.value, release: selectedRelease.value, config: uiConfig() });
}
function skip() {
    $1t.send('identifySkip', { index: currentIndex.value });
    results.value[currentIndex.value].state = 'unknown';
    dialog.value = false;
}
function loadJournals() {
    saveConfig();
    $1t.send('identifyJournals', { target: config.value.target });
}
function undo(j: any) {
    $q.dialog({
        title: 'Rückgängig machen',
        message: `${j.files} Dateien zurück an ihren alten Ort verschieben und die alten Tags exakt wiederherstellen?`,
        cancel: true,
    }).onOk(() => $1t.send('identifyUndo', { journal: j.path }));
}

onMounted(() => {
    if ($1t.settings.value.identify) {
        config.value = Object.assign(new IdentifyConfig(), $1t.settings.value.identify);
    }
    $1t.onIdentifyEvent = (json: any) => {
        switch (json.action) {
            case 'browse':
                if (json.context == 'idTarget') config.value.target = json.path;
                else config.value.path = json.path;
                break;
            case 'identifyStarted':
            case 'identifyLoaded':
                startPolling();
                break;
            case 'identifyStatus':
                const s = json.status;
                total.value = s.total;
                running.value = s.running;
                if (s.since == results.value.length) results.value.push(...s.results);
                if (!s.running && results.value.length >= s.done) {
                    clearInterval(timer);
                    timer = null;
                }
                break;
            case 'identifyApplied':
                applying.value = false;
                if (json.error) {
                    $1t.onError(json.error);
                    return;
                }
                results.value[json.index].applied = json.path;
                results.value[json.index].state = 'confirmed';
                $q.notify({ message: `Verschoben nach ${json.path}`, timeout: 3000 });
                dialog.value = false;
                break;
            case 'identifyJournals':
                journals.value = json.journals;
                if (!json.journals.length) $q.notify({ message: 'Keine Läufe im Zielordner', timeout: 2000 });
                break;
            case 'identifyUndone':
                $q.notify({ message: `${json.restored} Dateien wiederhergestellt, ${json.errors.length} Fehler`, timeout: 4000 });
                if (json.errors.length) $1t.onError(json.errors.join('\n'));
                loadJournals();
                break;
        }
    };
    // Pick up a run that is still going (e.g. after switching tabs)
    poll();
    startPolling();
});

onUnmounted(() => {
    clearInterval(timer);
    timer = null;
    $1t.onIdentifyEvent = () => {};
});
</script>

<style>
.id-wrapper {
    width: 100%;
    display: flex;
    justify-content: center;
}
.id-content {
    width: 100%;
    max-width: 1300px;
    padding: 0 24px;
}
.id-list {
    border: 1px solid #262828;
    border-radius: 4px;
}
</style>
