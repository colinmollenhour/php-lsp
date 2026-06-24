<?php

namespace App;

class Noise11
{
    private function compute(): int
    {
        return 11;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
