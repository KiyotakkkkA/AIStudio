import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiMagnify, mdiViewGridOutline } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import { awaitingData } from "../../ui/molecules/LoadingSkeleton";
import PageHeader from "../../ui/organisms/PageHeader";
import { CardListSkeleton } from "../../ui/organisms/PageSkeletons";
import { useStore } from "../../stores/useStore";
import GalleryCard from "./GalleryCard";
import { pdfFirstPage } from "./thumbnailImage";
import {
  GALLERY_EMPTY_DESCRIPTION,
  GALLERY_EMPTY_TITLE,
  GALLERY_NOTHING_FOUND,
  GALLERY_NOTHING_FOUND_DESCRIPTION,
  GALLERY_RETRY,
  GALLERY_SEARCH,
  GALLERY_SORT,
  GALLERY_SORTS,
  GALLERY_SUBTITLE,
  GALLERY_TITLE,
  type GallerySort,
} from "./galleryCopy";

function GalleryWorkspace() {
  const { gallery: store } = useStore();

  useEffect(() => {
    store.usePdfRenderer(pdfFirstPage);
    void store.load();
  }, [store]);

  const sections = store.sections;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        icon={mdiViewGridOutline}
        title={GALLERY_TITLE}
        subtitle={GALLERY_SUBTITLE}
        actions={
          <>
            <div className="w-64">
              <TextInput
                value={store.query}
                preset="search"
                placeholder={GALLERY_SEARCH}
                aria-label={GALLERY_SEARCH}
                onChange={(event) => store.setQuery(event.target.value)}
              />
            </div>
            <div className="w-44">
              <SelectInput
                label={GALLERY_SORT}
                value={store.sort}
                options={GALLERY_SORTS.map((sort) => ({ value: sort.value, label: sort.label }))}
                onChange={(value) => store.setSort(value as GallerySort)}
                menuPlacement="bottom-right"
              />
            </div>
          </>
        }
      />
      {store.error === null ? null : (
        <div
          role="alert"
          className="mx-6 mt-4 flex flex-none items-center gap-3 rounded-control bg-err-soft px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{store.error}</span>
          <Button tone="secondary" onClick={() => void store.load()}>
            {GALLERY_RETRY}
          </Button>
        </div>
      )}
      {awaitingData(store) ? (
        <div className="min-h-0 flex-1 px-6 py-5">
          <CardListSkeleton />
        </div>
      ) : store.isEmpty ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiViewGridOutline}
            title={GALLERY_EMPTY_TITLE}
            description={GALLERY_EMPTY_DESCRIPTION}
          />
        </div>
      ) : sections.length === 0 && store.error === null ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiMagnify}
            title={GALLERY_NOTHING_FOUND}
            description={GALLERY_NOTHING_FOUND_DESCRIPTION}
          />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-6 py-5">
          {sections.map((section) => (
            <section key={section.kind} aria-label={section.label} className="flex flex-col gap-3">
              <h2 className="m-0 flex items-baseline gap-2 text-[13.5px] font-semibold text-main-50">
                {section.label}
                <span className="text-[12px] font-normal text-main-500">
                  {section.items.length.toLocaleString("ru-RU")}
                </span>
              </h2>
              <ul className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(208px,1fr))] gap-3 p-0">
                {section.items.map((item) => (
                  <GalleryCard key={item.id} store={store} item={item} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}

export default observer(GalleryWorkspace);
